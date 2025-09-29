use thiserror::Error;

/// Opus codec errors.
#[derive(Debug, Error)]
pub enum Error {
    /// Placeholder for future packet parsing errors
    #[error("Invalid packet format")]
    InvalidPacket,

    /// Placeholder for future decoding errors
    #[error("Decoding failed")]
    DecodingFailed,

    /// Invalid frame length
    #[error("Invalid frame length: {0} bytes (max 1275)")]
    InvalidFrameLength(usize),

    /// Packet too short
    #[error("Packet too short: {0} bytes")]
    PacketTooShort(usize),
}

/// Result type for Opus operations.
pub type Result<T> = std::result::Result<T, Error>;
