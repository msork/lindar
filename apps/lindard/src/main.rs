use lindar_core::{NodeId, NodeInfo, OperatingSystem};
use lindar_network::{Discovery, PeerEvent};
use lindar_protocol::DEFAULT_LISTENING_PORT;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_owned());
    let node = NodeInfo {
        id: NodeId::new(hostname.clone()),
        hostname,
        operating_system: OperatingSystem::current(),
    };

    info!(
        node_id = %node.id,
        hostname = %node.hostname,
        operating_system = ?node.operating_system,
        "Lindar node started"
    );
    println!(
        "Lindar node: {} ({:?})",
        node.hostname, node.operating_system
    );

    let (discovery, mut peer_events) = Discovery::start(node, DEFAULT_LISTENING_PORT).await?;
    info!(port = DEFAULT_LISTENING_PORT, "LAN discovery started");

    loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result?;
                break;
            }
            event = peer_events.recv() => {
                match event {
                    Some(PeerEvent::Discovered(peer)) => info!(
                        node_id = %peer.node.id,
                        hostname = %peer.node.hostname,
                        operating_system = ?peer.node.operating_system,
                        port = peer.port,
                        "Lindar peer discovered"
                    ),
                    Some(PeerEvent::Updated { current, .. }) => info!(
                        node_id = %current.node.id,
                        hostname = %current.node.hostname,
                        operating_system = ?current.node.operating_system,
                        port = current.port,
                        "Lindar peer updated"
                    ),
                    Some(PeerEvent::Lost(peer)) => info!(
                        node_id = %peer.node.id,
                        hostname = %peer.node.hostname,
                        "Lindar peer lost"
                    ),
                    None => break,
                }
            }
        }
    }

    discovery.shutdown().await;
    info!("shutdown signal received");
    Ok(())
}
