//! AAC audio encoding using fdk-aac.
//!
//! Provides functions to create and use AAC encoders with MPEG-4 Low Complexity profile,
//! outputting ADTS-formatted AAC streams.

#![allow(clippy::module_name_repetitions)]

use fdk_aac::enc::{BitRate, ChannelMode, Encoder, EncoderParams, Transport};
use thiserror::Error;

use crate::EncodeInfo;

/// Errors that can occur during AAC encoding operations.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// Error from the underlying fdk-aac encoder
    #[error("Encoder error")]
    Encoder(fdk_aac::enc::EncoderError),
}

impl From<fdk_aac::enc::EncodeInfo> for EncodeInfo {
    fn from(value: fdk_aac::enc::EncodeInfo) -> Self {
        Self {
            output_size: value.output_size,
            input_consumed: value.input_consumed,
        }
    }
}

impl From<fdk_aac::enc::EncoderError> for EncoderError {
    fn from(value: fdk_aac::enc::EncoderError) -> Self {
        Self::Encoder(value)
    }
}

/// Creates a new AAC encoder with default settings.
///
/// Configures the encoder for MPEG-4 Low Complexity AAC at 44.1kHz stereo with
/// very high variable bitrate, outputting ADTS format.
///
/// # Errors
///
/// * If the encoder fails to initialize
pub fn encoder_aac() -> Result<Encoder, EncoderError> {
    let encoder = Encoder::new(EncoderParams {
        audio_object_type: fdk_aac::enc::AudioObjectType::Mpeg4LowComplexity,
        bit_rate: BitRate::VbrVeryHigh,
        sample_rate: 44_100,
        transport: Transport::Adts,
        channels: ChannelMode::Stereo,
    })?;
    Ok(encoder)
}

/// Encodes PCM audio samples to AAC format.
///
/// # Errors
///
/// * If the encoder fails to encode the input bytes
pub fn encode_aac(
    encoder: &Encoder,
    input: &[i16],
    buf: &mut [u8],
) -> Result<EncodeInfo, EncoderError> {
    let info = encoder.encode(input, buf)?;

    Ok(info.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_encoder_creation() {
        let result = encoder_aac();
        assert!(result.is_ok(), "AAC encoder should initialize successfully");
    }

    #[test_log::test]
    fn test_encode_aac_basic() {
        let encoder = encoder_aac().expect("Failed to create encoder");

        // Create a buffer of PCM samples (2048 samples for stereo)
        let input: Vec<i16> = vec![0; 2048];
        let mut output = vec![0u8; 8192];

        let result = encode_aac(&encoder, &input, &mut output);

        assert!(result.is_ok(), "Encoding should succeed");
        let info = result.unwrap();

        assert!(info.output_size > 0, "Should produce output");
        assert!(info.input_consumed > 0, "Should consume input");
    }
}
