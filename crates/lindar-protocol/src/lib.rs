//! Serializable message definitions shared by Lindar peers.

use lindar_core::NodeInfo;
use serde::{Deserialize, Serialize};

/// Version of the initial Lindar wire format.
pub const PROTOCOL_VERSION: u16 = 1;

/// Top-level messages exchanged by Lindar peers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Message {
    /// Announces a peer and the protocol version it supports.
    Identify { protocol_version: u16, node: NodeInfo },
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

