//! FLAC audio encoding.
//!
//! Provides functions to create and use FLAC encoders with configurable block sizes
//! for lossless audio compression.

#![allow(clippy::module_name_repetitions, clippy::struct_field_names)]

use flacenc::{
    bitsink::{ByteSink, MemSink},
    component::BitRepr as _,
    error::{Verified, Verify as _},
    source::MemSource,
};
use thiserror::Error;

use crate::EncodeInfo;

/// FLAC encoder with internal state for streaming encoding.
///
/// Maintains a byte sink for output and position tracking across multiple encode calls.
pub struct Encoder {
    /// Internal byte sink for accumulating encoded output
    sink: ByteSink,
    /// Current position in the output sink (bytes written so far)
    pos: usize,
    /// Verified FLAC encoder configuration
    encoder: Verified<flacenc::config::Encoder>,
}

/// Errors that can occur during FLAC encoding operations.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// Error writing encoder output
    #[error(transparent)]
    Output(#[from] flacenc::error::OutputError<MemSink<u8>>),
    /// Error verifying encoder configuration
    #[error(transparent)]
    Verify(#[from] flacenc::error::VerifyError),
    /// Error during encoding
    #[error("Encode error")]
    Encode(flacenc::error::EncodeError),
}

impl From<flacenc::error::EncodeError> for EncoderError {
    fn from(value: flacenc::error::EncodeError) -> Self {
        Self::Encode(value)
    }
}

/// Creates a new FLAC encoder with default settings.
///
/// Configures the encoder with a block size of 512 samples for streaming encoding.
///
/// # Errors
///
/// * If the encoder fails to initialize
pub fn encoder_flac() -> Result<Encoder, EncoderError> {
    let mut encoder = flacenc::config::Encoder::default();
    encoder.block_size = 512;
    let encoder = encoder.into_verified().map_err(|e| e.1)?;

    let sink = flacenc::bitsink::ByteSink::new();

    Ok(Encoder {
        sink,
        pos: 0,
        encoder,
    })
}

/// Encodes PCM audio samples to FLAC format.
///
/// # Errors
///
/// * If the encoder fails to encode the input bytes
pub fn encode_flac(
    encoder: &mut Encoder,
    input: &[i32],
    buf: &mut [u8],
) -> Result<EncodeInfo, EncoderError> {
    let (channels, bits_per_sample, sample_rate) = (2, 16, 44100);

    let source = MemSource::from_samples(input, channels, bits_per_sample, sample_rate);

    let stream = flacenc::encode_with_fixed_block_size(
        &encoder.encoder,
        source,
        encoder.encoder.block_size,
    )?;

    stream.write(&mut encoder.sink)?;

    let bytes = &encoder.sink.as_slice()[encoder.pos..];
    buf[..bytes.len()].copy_from_slice(bytes);
    encoder.pos += bytes.len();

    log::debug!(
        "Encoded output_size={} input_consumed={}",
        bytes.len(),
        input.len()
    );

    Ok(EncodeInfo {
        output_size: bytes.len(),
        input_consumed: input.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_encoder_creation() {
        let result = encoder_flac();
        assert!(
            result.is_ok(),
            "FLAC encoder should initialize successfully"
        );

        let encoder = result.unwrap();
        assert_eq!(encoder.pos, 0, "Initial position should be 0");
        assert_eq!(encoder.encoder.block_size, 512, "Block size should be 512");
    }

    #[test_log::test]
    fn test_encode_position_tracking_single_call() {
        let mut encoder = encoder_flac().expect("Failed to create encoder");

        // Create a full block of samples (512 samples * 2 channels = 1024 samples)
        let input: Vec<i32> = vec![0; 1024];
        let mut output = vec![0u8; 8192];

        let initial_pos = encoder.pos;
        let result = encode_flac(&mut encoder, &input, &mut output);

        assert!(result.is_ok(), "Encoding should succeed");
        let info = result.unwrap();

        assert!(info.output_size > 0, "Should produce output");
        assert_eq!(info.input_consumed, input.len(), "Should consume all input");
        assert_eq!(
            encoder.pos,
            initial_pos + info.output_size,
            "Position should advance by output size"
        );
    }

    #[test_log::test]
    fn test_encode_position_tracking_multiple_calls() {
        let mut encoder = encoder_flac().expect("Failed to create encoder");

        // First encode
        let input1: Vec<i32> = vec![100; 1024];
        let mut output1 = vec![0u8; 8192];
        let result1 = encode_flac(&mut encoder, &input1, &mut output1);
        assert!(result1.is_ok());
        let info1 = result1.unwrap();
        let pos_after_first = encoder.pos;

        // Second encode
        let input2: Vec<i32> = vec![200; 1024];
        let mut output2 = vec![0u8; 8192];
        let result2 = encode_flac(&mut encoder, &input2, &mut output2);
        assert!(result2.is_ok());
        let info2 = result2.unwrap();

        // Verify position tracking across multiple calls
        assert_eq!(
            pos_after_first, info1.output_size,
            "Position after first encode"
        );
        assert_eq!(
            encoder.pos,
            info1.output_size + info2.output_size,
            "Position should accumulate"
        );
    }

    #[test_log::test]
    fn test_encode_empty_input() {
        let mut encoder = encoder_flac().expect("Failed to create encoder");

        let input: Vec<i32> = vec![];
        let mut output = vec![0u8; 8192];

        let result = encode_flac(&mut encoder, &input, &mut output);

        // Empty input should still be handled (may produce header/metadata)
        assert!(result.is_ok());
    }

    #[test_log::test]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn test_encode_flac_varying_samples() {
        let mut encoder = encoder_flac().expect("Failed to create encoder");

        // Generate a stereo audio pattern (512 samples * 2 channels = 1024 samples)
        // This matches the block size of 512 samples
        let block_size = 512;
        let channels = 2;
        let mut input: Vec<i32> = Vec::with_capacity(block_size * channels);

        for i in 0..block_size {
            // Generate sine wave pattern for 16-bit audio range
            let t = i as f32 / 44100.0;
            let left = ((t * 440.0 * std::f32::consts::TAU).sin() * 16000.0) as i32;
            let right = ((t * 550.0 * std::f32::consts::TAU).sin() * 16000.0) as i32;
            input.push(left);
            input.push(right);
        }

        let mut output = vec![0u8; 16384];
        let result = encode_flac(&mut encoder, &input, &mut output);

        assert!(result.is_ok(), "Encoding varying samples should succeed");
        let info = result.unwrap();

        assert!(info.output_size > 0, "Should produce output");
        assert_eq!(info.input_consumed, input.len(), "Should consume all input");
    }

    #[test_log::test]
    fn test_encode_flac_output_buffer_content() {
        let mut encoder = encoder_flac().expect("Failed to create encoder");

        // Create a simple input
        let input: Vec<i32> = vec![1000, -1000, 500, -500]; // Simple stereo pair
        let mut output = vec![0u8; 8192];

        let result = encode_flac(&mut encoder, &input, &mut output);
        assert!(result.is_ok());
        let info = result.unwrap();

        // Verify that output contains actual data (not just zeros)
        let output_slice = &output[..info.output_size];
        assert!(
            output_slice.iter().any(|&b| b != 0),
            "Output should contain non-zero bytes"
        );
    }
}
