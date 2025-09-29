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
}

/// Result type for Opus operations.
pub type Result<T> = std::result::Result<T, Error>;
