//! Error types for Opus codec operations.
//!
//! This module defines the [`enum@Error`] enum for various failure modes in Opus
//! packet parsing and decoding operations.

use thiserror::Error;

/// Opus codec errors.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid packet structure according to RFC 6716.
    ///
    /// Returned when packet parsing encounters invalid frame counts, incorrect
    /// frame sizes, or malformed packet structures.
    #[error("Invalid packet format")]
    InvalidPacket,

    /// Decoding operation failed.
    ///
    /// Returned when libopus encounters errors during audio decoding.
    #[error("Decoding failed")]
    DecodingFailed,

    /// Frame length exceeds maximum allowed size.
    ///
    /// RFC 6716 specifies a maximum frame length of 1275 bytes. This error
    /// is returned when a frame length encoding results in a larger value.
    #[error("Invalid frame length: {0} bytes (max 1275)")]
    InvalidFrameLength(usize),

    /// Packet does not contain enough bytes for the declared structure.
    ///
    /// Returned when packet parsing requires more bytes than are available
    /// in the input data.
    #[error("Packet too short: {0} bytes")]
    PacketTooShort(usize),

    /// Error from the underlying libopus decoder.
    ///
    /// Wraps errors returned by the audiopus library during decoder operations.
    #[error("Opus decoder error: {0}")]
    DecoderError(#[from] audiopus::Error),
}

/// Result type for Opus operations.
pub type Result<T> = std::result::Result<T, Error>;
