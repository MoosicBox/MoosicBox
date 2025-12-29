//! P2P system error types
//!
//! This module defines the core error types used throughout the P2P system.
//! The error enum will be extended with more specific variants as the
//! implementation grows.

use thiserror::Error;

/// P2P system error types
///
/// This enum will be extended with more specific error variants
/// as the implementation grows. Currently contains minimal errors
/// needed for Phase 3 trait implementations.
#[derive(Debug, Clone, Error)]
pub enum P2PError {
    /// Generic network error (will be refined in later phases)
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Connection-related errors
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Node not found during discovery
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Generic I/O error
    #[error("I/O error: {0}")]
    IoError(String),
    // TODO: Phase 4 - Add more specific error types:
    // - Timeout errors
    // - Invalid node ID errors
    // - Protocol-specific errors
    // - Serialization errors
}

/// Convenience type alias for P2P operations that may fail.
///
/// This alias wraps `Result` with [`P2PError`] as the error type,
/// providing a consistent error handling pattern across the crate.
pub type P2PResult<T> = Result<T, P2PError>;
