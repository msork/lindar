//! Networking boundary for transport and peer communication.
//!
//! This crate intentionally contains no transport implementation yet. It owns
//! that boundary so the daemon and protocol types can evolve independently.

/// Errors exposed by future transport operations.
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// An operating-system I/O operation failed.
    #[error("network I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
