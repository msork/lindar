//! Shared, platform-neutral domain types for Lindar.

use serde::{Deserialize, Serialize};

/// Stable identifier for a Lindar node.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct NodeId(String);

impl NodeId {
    /// Creates a node identifier from its serialized representation.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Operating system running a Lindar node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Linux,
    MacOs,
    Other,
}

impl OperatingSystem {
    /// Detects the current host OS without depending on platform APIs.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

/// Information advertised by a Lindar node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NodeInfo {
    pub id: NodeId,
    pub hostname: String,
    pub operating_system: OperatingSystem,
}

/// Position and size of a display in a node's desktop coordinate space.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisplayGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Basic information about one display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub geometry: DisplayGeometry,
    pub primary: bool,
}

#[cfg(test)]
mod tests {
    use super::{DisplayGeometry, DisplayInfo, NodeId, NodeInfo, OperatingSystem};

    #[test]
    fn node_id_round_trips_through_serde() {
        let id = NodeId::new("node-a");
        let encoded = serde_json::to_string(&id).expect("serialize node id");
        let decoded: NodeId = serde_json::from_str(&encoded).expect("deserialize node id");
        assert_eq!(decoded, id);
        assert_eq!(decoded.as_str(), "node-a");
    }

    #[test]
    fn node_info_contains_platform_neutral_display_geometry_types() {
        let info = NodeInfo {
            id: NodeId::new("node-a"),
            hostname: "host-a".to_owned(),
            operating_system: OperatingSystem::current(),
        };
        let display = DisplayInfo {
            id: "display-1".to_owned(),
            name: "Main display".to_owned(),
            geometry: DisplayGeometry { x: -10, y: 0, width: 1920, height: 1080 },
            primary: true,
        };
        assert_eq!(info.id.to_string(), "node-a");
        assert_eq!(display.geometry.x, -10);
        assert!(display.primary);
    }
}

