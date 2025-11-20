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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_messages() {
        assert_eq!(Error::InvalidPacket.to_string(), "Invalid packet format");
        assert_eq!(Error::DecodingFailed.to_string(), "Decoding failed");
        assert_eq!(
            Error::InvalidFrameLength(1300).to_string(),
            "Invalid frame length: 1300 bytes (max 1275)"
        );
        assert_eq!(
            Error::PacketTooShort(5).to_string(),
            "Packet too short: 5 bytes"
        );
    }

    #[test]
    fn test_invalid_frame_length_boundary() {
        let err = Error::InvalidFrameLength(1275);
        assert_eq!(
            err.to_string(),
            "Invalid frame length: 1275 bytes (max 1275)"
        );

        let err = Error::InvalidFrameLength(1276);
        assert_eq!(
            err.to_string(),
            "Invalid frame length: 1276 bytes (max 1275)"
        );
    }

    #[test]
    fn test_decoder_error_from_audiopus() {
        use audiopus::Error as AudiopusError;

        // Create an audiopus error using a known variant
        let audiopus_err = AudiopusError::InvalidApplication;
        let opus_err: Error = audiopus_err.into();

        match opus_err {
            Error::DecoderError(_) => (),
            _ => panic!("Expected DecoderError variant"),
        }
    }
}
