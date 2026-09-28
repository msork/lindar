//! Serializable message definitions shared by Lindar peers.

use lindar_core::NodeInfo;
use serde::{Deserialize, Serialize};

/// Version of the initial Lindar wire format.
pub const PROTOCOL_VERSION: u16 = 1;

/// Default port advertised by the Lindar daemon for its future peer endpoint.
pub const DEFAULT_LISTENING_PORT: u16 = 43120;

/// DNS-SD service type used to find Lindar nodes on the local network.
pub const MDNS_SERVICE_TYPE: &str = "_lindar._tcp.local.";

/// TXT record key containing the advertised node identifier.
pub const TXT_NODE_ID: &str = "node_id";
/// TXT record key containing the advertised host name.
pub const TXT_HOSTNAME: &str = "hostname";
/// TXT record key containing the advertised operating system.
pub const TXT_OS: &str = "os";
/// TXT record key containing the Lindar protocol version.
pub const TXT_PROTOCOL_VERSION: &str = "protocol_version";

/// Top-level messages exchanged by Lindar peers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Message {
    /// Announces a peer and the protocol version it supports.
    Identify {
        protocol_version: u16,
        node: NodeInfo,
    },
}

#[cfg(test)]
mod tests {
    use super::{Message, PROTOCOL_VERSION};
    use lindar_core::{NodeId, NodeInfo, OperatingSystem};

    #[test]
    fn identify_message_round_trips_as_json() {
        let message = Message::Identify {
            protocol_version: PROTOCOL_VERSION,
            node: NodeInfo {
                id: NodeId::new("node-a"),
                hostname: "host-a".to_owned(),
                operating_system: OperatingSystem::Linux,
            },
        };
        let encoded = serde_json::to_string(&message).expect("serialize message");
        let decoded: Message = serde_json::from_str(&encoded).expect("deserialize message");
        assert_eq!(decoded, message);
    }
}
