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
        assert_eq!(
            info.input_consumed, 2048,
            "Should consume all input samples"
        );
    }

    #[test_log::test]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn test_encode_aac_varying_samples() {
        let encoder = encoder_aac().expect("Failed to create encoder");

        // Generate a stereo sine wave pattern (2048 total samples)
        let sample_count = 2048;
        let mut input: Vec<i16> = Vec::with_capacity(sample_count);

        for i in 0..sample_count / 2 {
            // Generate sine wave for left and right channels
            let t = i as f32 / 44100.0;
            let left = ((t * 440.0 * std::f32::consts::TAU).sin() * 16000.0) as i16;
            let right = ((t * 550.0 * std::f32::consts::TAU).sin() * 16000.0) as i16;
            input.push(left);
            input.push(right);
        }

        let mut output = vec![0u8; 8192];
        let result = encode_aac(&encoder, &input, &mut output);

        assert!(result.is_ok(), "Encoding varying samples should succeed");
        let info = result.unwrap();

        assert!(info.output_size > 0, "Should produce output");
        assert_eq!(
            info.input_consumed, sample_count,
            "Should consume all input samples"
        );
    }

    #[test_log::test]
    fn test_encode_aac_multiple_calls() {
        let encoder = encoder_aac().expect("Failed to create encoder");

        // First encode call
        let input1: Vec<i16> = vec![1000; 2048];
        let mut output1 = vec![0u8; 8192];
        let result1 = encode_aac(&encoder, &input1, &mut output1);
        assert!(result1.is_ok(), "First encode should succeed");

        // Second encode call with same encoder
        let input2: Vec<i16> = vec![-1000; 2048];
        let mut output2 = vec![0u8; 8192];
        let result2 = encode_aac(&encoder, &input2, &mut output2);
        assert!(result2.is_ok(), "Second encode should succeed");

        let info1 = result1.unwrap();
        let info2 = result2.unwrap();

        // Both calls should produce output and consume all input
        assert!(info1.output_size > 0, "First call should produce output");
        assert!(info2.output_size > 0, "Second call should produce output");
        assert_eq!(
            info1.input_consumed, 2048,
            "First call should consume all input samples"
        );
        assert_eq!(
            info2.input_consumed, 2048,
            "Second call should consume all input samples"
        );
    }

    #[test_log::test]
    fn test_encode_aac_empty_input() {
        let encoder = encoder_aac().expect("Failed to create encoder");

        let input: Vec<i16> = vec![];
        let mut output = vec![0u8; 8192];

        let result = encode_aac(&encoder, &input, &mut output);

        // Empty input should be handled - may produce output due to encoder buffering
        assert!(result.is_ok(), "Empty input should be handled");
        let info = result.unwrap();
        assert_eq!(info.input_consumed, 0, "Should consume zero input samples");
    }
}
