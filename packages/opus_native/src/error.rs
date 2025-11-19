//! Error types for Opus decoder operations.
//!
//! This module defines the error types returned by the Opus decoder implementation.
//! All errors are defined using the `thiserror` crate for ergonomic error handling.

use thiserror::Error;

/// Result type alias for Opus operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types returned by Opus decoder operations
#[derive(Debug, Error)]
pub enum Error {
    /// Packet violates RFC 6716 structure requirements
    #[error("Invalid packet structure: {0}")]
    InvalidPacket(String),

    /// Configuration not supported by this implementation
    #[error("Unsupported configuration: {0}")]
    Unsupported(String),

    /// Failed to initialize decoder
    #[error("Decoder initialization failed: {0}")]
    InitFailed(String),

    /// Decoding operation failed
    #[error("Decode operation failed: {0}")]
    DecodeFailed(String),

    /// Range decoder error
    #[error("Range decoder error: {0}")]
    RangeDecoder(String),

    /// SILK decoder error
    #[error("SILK decoder error: {0}")]
    SilkDecoder(String),

    /// CELT decoder error
    #[error("CELT decoder error: {0}")]
    CeltDecoder(String),

    /// Invalid or unsupported sample rate
    #[error("Invalid sample rate: {0}")]
    InvalidSampleRate(String),

    /// Invalid resampler delay value
    #[error("Invalid resampler delay: {0}")]
    InvalidDelay(String),

    /// Mode not enabled via feature flags
    #[error("Unsupported mode: {0}")]
    UnsupportedMode(String),

    /// Invalid Opus mode
    #[error("Invalid mode: {0}")]
    InvalidMode(String),
}
