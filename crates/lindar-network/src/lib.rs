//! LAN discovery for Lindar peers.

use std::{collections::HashMap, sync::Arc};

use lindar_core::{NodeId, NodeInfo, OperatingSystem};
use lindar_protocol::{
    MDNS_SERVICE_TYPE, PROTOCOL_VERSION, TXT_HOSTNAME, TXT_NODE_ID, TXT_OS, TXT_PROTOCOL_VERSION,
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::{mpsc, oneshot, RwLock};

/// A validated Lindar node discovered on the local network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredPeer {
    pub node: NodeInfo,
    pub protocol_version: u16,
    pub port: u16,
    /// The DNS-SD service instance name, useful for correlating removal events.
    pub service_name: String,
}

/// Lifecycle changes in the discovered-peer table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PeerEvent {
    Discovered(DiscoveredPeer),
    Updated {
        previous: DiscoveredPeer,
        current: DiscoveredPeer,
    },
    Lost(DiscoveredPeer),
}

/// Errors returned while starting or running LAN discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("invalid discovery configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("mDNS operation failed: {0}")]
    Mdns(String),
}

/// Active advertisement and browser for Lindar peers.
pub struct Discovery {
    peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Discovery {
    /// Advertises `node` and begins discovering compatible Lindar peers.
    ///
    /// The returned receiver emits peer lifecycle changes. The local node is
    /// filtered by its `NodeId`, even when multicast loopback is enabled.
    pub async fn start(
        node: NodeInfo,
        listening_port: u16,
    ) -> Result<(Self, mpsc::Receiver<PeerEvent>), DiscoveryError> {
        validate_local_metadata(&node, listening_port)?;

        let daemon =
            ServiceDaemon::new().map_err(|error| DiscoveryError::Mdns(error.to_string()))?;
        let service_info = make_service_info(&node, listening_port)?;
        let service_fullname = service_info.get_fullname().to_owned();
        daemon
            .register(service_info)
            .map_err(|error| DiscoveryError::Mdns(error.to_string()))?;
        let browse = daemon
            .browse(MDNS_SERVICE_TYPE)
            .map_err(|error| DiscoveryError::Mdns(error.to_string()))?;

        let peers = Arc::new(RwLock::new(HashMap::new()));
        let (peer_tx, peer_rx) = mpsc::channel(32);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let local_id = node.id.clone();
        let task_peers = Arc::clone(&peers);
        let task_daemon = daemon.clone();
        let task = tokio::spawn(async move {
            run_browser(
                browse,
                task_daemon,
                service_fullname,
                local_id,
                task_peers,
                peer_tx,
                shutdown_rx,
            )
            .await;
        });

        Ok((
            Self {
                peers,
                shutdown: Some(shutdown_tx),
                task,
            },
            peer_rx,
        ))
    }

    /// Returns the current discovered peers.
    pub async fn peers(&self) -> Vec<DiscoveredPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Stops browsing, withdraws the advertisement, and waits for cleanup.
    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

async fn run_browser(
    browse: mdns_sd::Receiver<ServiceEvent>,
    daemon: ServiceDaemon,
    service_fullname: String,
    local_id: NodeId,
    peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
    events: mpsc::Sender<PeerEvent>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            received = browse.recv_async() => {
                match received {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        update_peer(&info, &local_id, &peers, &events).await;
                    }
                    Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                        remove_peer(&fullname, &peers, &events).await;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
    }

    let _ = daemon.stop_browse(MDNS_SERVICE_TYPE);
    let _ = daemon.unregister(&service_fullname);
    let _ = daemon.shutdown();
}

async fn update_peer(
    info: &ServiceInfo,
    local_id: &NodeId,
    peers: &RwLock<HashMap<String, DiscoveredPeer>>,
    events: &mpsc::Sender<PeerEvent>,
) {
    let service_name = info.get_fullname().to_owned();
    let parsed = parse_metadata(
        info.get_property_val_str(TXT_NODE_ID),
        info.get_property_val_str(TXT_HOSTNAME),
        info.get_property_val_str(TXT_OS),
        info.get_property_val_str(TXT_PROTOCOL_VERSION),
        info.get_port(),
        service_name,
    );
    let Ok(peer) = parsed else {
        remove_peer(info.get_fullname(), peers, events).await;
        return;
    };
    if &peer.node.id == local_id {
        remove_peer(info.get_fullname(), peers, events).await;
        return;
    }

    let event = {
        let mut table = peers.write().await;
        match table.insert(peer.service_name.clone(), peer.clone()) {
            Some(previous) if previous != peer => Some(PeerEvent::Updated {
                previous,
                current: peer,
            }),
            Some(_) => None,
            None => Some(PeerEvent::Discovered(peer)),
        }
    };
    send_event(events, event).await;
}

async fn remove_peer(
    service_name: &str,
    peers: &RwLock<HashMap<String, DiscoveredPeer>>,
    events: &mpsc::Sender<PeerEvent>,
) {
    let peer = peers.write().await.remove(service_name);
    send_event(events, peer.map(PeerEvent::Lost)).await;
}

async fn send_event(events: &mpsc::Sender<PeerEvent>, event: Option<PeerEvent>) {
    if let Some(event) = event {
        let _ = events.send(event).await;
    }
}

fn make_service_info(node: &NodeInfo, port: u16) -> Result<ServiceInfo, DiscoveryError> {
    let mut properties = HashMap::new();
    properties.insert(TXT_NODE_ID.to_owned(), node.id.to_string());
    properties.insert(TXT_HOSTNAME.to_owned(), node.hostname.clone());
    properties.insert(TXT_OS.to_owned(), os_name(node.operating_system).to_owned());
    properties.insert(
        TXT_PROTOCOL_VERSION.to_owned(),
        PROTOCOL_VERSION.to_string(),
    );

    ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        &instance_label(node.id.as_str()),
        &host_label(&node.hostname),
        "",
        port,
        properties,
    )
    .map(ServiceInfo::enable_addr_auto)
    .map_err(|error| DiscoveryError::Mdns(error.to_string()))
}

fn validate_local_metadata(node: &NodeInfo, port: u16) -> Result<(), DiscoveryError> {
    if port == 0 {
        return Err(DiscoveryError::InvalidConfiguration(
            "listening port must be non-zero",
        ));
    }
    validate_text(node.id.as_str()).map_err(|_| {
        DiscoveryError::InvalidConfiguration(
            "node identifier must be non-empty and at most 255 bytes",
        )
    })?;
    validate_text(&node.hostname).map_err(|_| {
        DiscoveryError::InvalidConfiguration("hostname must be non-empty and at most 255 bytes")
    })?;
    Ok(())
}

fn parse_metadata(
    node_id: Option<&str>,
    hostname: Option<&str>,
    operating_system: Option<&str>,
    protocol_version: Option<&str>,
    port: u16,
    service_name: String,
) -> Result<DiscoveredPeer, MetadataError> {
    let node_id = required_text(node_id)?;
    let hostname = required_text(hostname)?;
    let protocol_version = required_text(protocol_version)?
        .parse::<u16>()
        .map_err(|_| MetadataError::InvalidProtocolVersion)?;
    if protocol_version != PROTOCOL_VERSION {
        return Err(MetadataError::UnsupportedProtocolVersion(protocol_version));
    }
    if port == 0 {
        return Err(MetadataError::InvalidPort);
    }

    let operating_system = match required_text(operating_system)?.as_str() {
        "linux" => OperatingSystem::Linux,
        "macos" => OperatingSystem::MacOs,
        "other" => OperatingSystem::Other,
        _ => return Err(MetadataError::InvalidOperatingSystem),
    };

    Ok(DiscoveredPeer {
        node: NodeInfo {
            id: NodeId::new(node_id),
            hostname,
            operating_system,
        },
        protocol_version,
        port,
        service_name,
    })
}

fn required_text(value: Option<&str>) -> Result<String, MetadataError> {
    let value = value.ok_or(MetadataError::MissingField)?;
    validate_text(value)?;
    Ok(value.to_owned())
}

fn validate_text(value: &str) -> Result<(), MetadataError> {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(MetadataError::InvalidText);
    }
    Ok(())
}

fn os_name(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Linux => "linux",
        OperatingSystem::MacOs => "macos",
        OperatingSystem::Other => "other",
    }
}

fn instance_label(value: &str) -> String {
    dns_label(value)
}

fn host_label(value: &str) -> String {
    format!("{}.local.", dns_label(value))
}

fn dns_label(value: &str) -> String {
    let mut label: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(63)
        .collect();
    while label.starts_with('-') {
        label.remove(0);
    }
    while label.ends_with('-') {
        label.pop();
    }
    if label.is_empty() {
        "lindar".to_owned()
    } else {
        label
    }
}

#[derive(Debug, thiserror::Error)]
enum MetadataError {
    #[error("required metadata field is missing")]
    MissingField,
    #[error("metadata text is empty, too long, or contains control characters")]
    InvalidText,
    #[error("protocol version is malformed")]
    InvalidProtocolVersion,
    #[error("protocol version {0} is unsupported")]
    UnsupportedProtocolVersion(u16),
    #[error("operating system is invalid")]
    InvalidOperatingSystem,
    #[error("port is invalid")]
    InvalidPort,
}

#[cfg(test)]
mod tests {
    use super::{parse_metadata, DiscoveryError, MetadataError};
    use lindar_core::OperatingSystem;
    use lindar_protocol::PROTOCOL_VERSION;

    fn valid_metadata() -> (&'static str, &'static str, &'static str, String) {
        ("node-a", "host-a", "linux", PROTOCOL_VERSION.to_string())
    }

    #[test]
    fn parses_valid_peer_metadata() {
        let (node_id, hostname, os, version) = valid_metadata();
        let peer = parse_metadata(
            Some(node_id),
            Some(hostname),
            Some(os),
            Some(&version),
            43120,
            "node-a._lindar._tcp.local.".to_owned(),
        )
        .expect("valid metadata");
        assert_eq!(peer.node.id.as_str(), "node-a");
        assert_eq!(peer.node.hostname, "host-a");
        assert_eq!(peer.node.operating_system, OperatingSystem::Linux);
        assert_eq!(peer.protocol_version, PROTOCOL_VERSION);
        assert_eq!(peer.port, 43120);
    }

    #[test]
    fn rejects_missing_malformed_and_unsupported_metadata() {
        let (node_id, hostname, os, version) = valid_metadata();
        assert!(matches!(
            parse_metadata(
                None,
                Some(hostname),
                Some(os),
                Some(&version),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::MissingField)
        ));
        assert!(matches!(
            parse_metadata(
                Some(node_id),
                Some(hostname),
                Some("unknown"),
                Some(&version),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::InvalidOperatingSystem)
        ));
        assert!(matches!(
            parse_metadata(
                Some(node_id),
                Some(hostname),
                Some(os),
                Some("bad"),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::InvalidProtocolVersion)
        ));
        assert!(matches!(
            parse_metadata(
                Some(node_id),
                Some(hostname),
                Some(os),
                Some("999"),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::UnsupportedProtocolVersion(999))
        ));
    }

    #[test]
    fn rejects_empty_or_oversized_text_and_zero_port() {
        let (node_id, hostname, os, version) = valid_metadata();
        assert!(matches!(
            parse_metadata(
                Some(""),
                Some(hostname),
                Some(os),
                Some(&version),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::InvalidText)
        ));
        let oversized = "x".repeat(256);
        assert!(matches!(
            parse_metadata(
                Some(&oversized),
                Some(hostname),
                Some(os),
                Some(&version),
                10,
                "svc".to_owned()
            ),
            Err(MetadataError::InvalidText)
        ));
        assert!(matches!(
            parse_metadata(
                Some(node_id),
                Some(hostname),
                Some(os),
                Some(&version),
                0,
                "svc".to_owned()
            ),
            Err(MetadataError::InvalidPort)
        ));
    }

    #[test]
    fn local_configuration_rejects_zero_port() {
        let err = super::validate_local_metadata(
            &lindar_core::NodeInfo {
                id: lindar_core::NodeId::new("node-a"),
                hostname: "host-a".to_owned(),
                operating_system: OperatingSystem::Linux,
            },
            0,
        );
        assert!(matches!(err, Err(DiscoveryError::InvalidConfiguration(_))));
    }
}
