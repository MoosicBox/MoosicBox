//! MP3 audio encoding using LAME.
//!
//! Provides functions to create and use MP3 encoders with configurable bitrate and quality
//! settings, including ID3 tag support.

#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::EncodeInfo;

/// Errors that can occur during MP3 encoding operations.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// Error during MP3 encoding
    #[error("Encoder error")]
    Encoder(mp3lame_encoder::EncodeError),
    /// Error setting ID3 tags
    #[error("Encoder error")]
    Id3Tag(mp3lame_encoder::Id3TagError),
    /// Error building the encoder
    #[error("Build error")]
    Build(mp3lame_encoder::BuildError),
}

impl From<mp3lame_encoder::EncodeError> for EncoderError {
    fn from(value: mp3lame_encoder::EncodeError) -> Self {
        Self::Encoder(value)
    }
}

impl From<mp3lame_encoder::Id3TagError> for EncoderError {
    fn from(value: mp3lame_encoder::Id3TagError) -> Self {
        Self::Id3Tag(value)
    }
}

impl From<mp3lame_encoder::BuildError> for EncoderError {
    fn from(value: mp3lame_encoder::BuildError) -> Self {
        Self::Build(value)
    }
}

/// Creates a new MP3 encoder with default settings.
///
/// Configures the encoder for 320kbps stereo at 44.1kHz with best quality settings
/// and default ID3 tags.
///
/// # Panics
///
/// * If the `mp3lame_encoder::Builder` fails to initialize.
/// * If fails to set the number of channels
/// * If fails to set the sample rate
/// * If fails to set the bit rate
/// * If fails to set the quality
///
/// # Errors
///
/// * If the encoder fails to initialize
pub fn encoder_mp3() -> Result<mp3lame_encoder::Encoder, EncoderError> {
    use mp3lame_encoder::{Builder, Id3Tag};

    let mut mp3_encoder = Builder::new().expect("Create LAME builder");
    mp3_encoder.set_num_channels(2).expect("set channels");
    mp3_encoder
        .set_sample_rate(44_100)
        .expect("set sample rate");
    mp3_encoder
        .set_brate(mp3lame_encoder::Bitrate::Kbps320)
        .expect("set brate");
    mp3_encoder
        .set_quality(mp3lame_encoder::Quality::Best)
        .expect("set quality");
    mp3_encoder.set_id3_tag(Id3Tag {
        album_art: &[],
        title: b"My title",
        artist: &[],
        album: b"My album",
        year: b"Current year",
        comment: b"Just my comment",
    })?;
    let mp3_encoder = mp3_encoder.build()?;
    Ok(mp3_encoder)
}

/// Encodes PCM audio samples to MP3 format.
///
/// Returns the encoded MP3 data and encoding information including output size and
/// input samples consumed. The encoder flushes remaining data to ensure complete
/// encoding of the input buffer.
///
/// # Errors
///
/// * If the encoder fails to encode the input bytes
pub fn encode_mp3(
    encoder: &mut mp3lame_encoder::Encoder,
    input: &[i16],
) -> Result<(Vec<u8>, EncodeInfo), EncoderError> {
    use mp3lame_encoder::{FlushNoGap, InterleavedPcm};

    //use actual PCM data
    let interleaved = InterleavedPcm(input);

    let mut mp3_out_buffer = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(
        interleaved.0.len(),
    ));
    let encoded_size = encoder.encode(interleaved, mp3_out_buffer.spare_capacity_mut())?;
    // SAFETY: The encoder writes to spare_capacity_mut() and returns the number of bytes
    // written. We use saturating_add to prevent integer overflow - the encoded_size cannot
    // exceed spare capacity, so overflow is not expected, but saturating protects against
    // potential edge cases.
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().saturating_add(encoded_size));
    }

    let encoded_size = encoder.flush::<FlushNoGap>(mp3_out_buffer.spare_capacity_mut())?;
    // SAFETY: Same as above - flush writes to spare capacity and returns bytes written.
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().saturating_add(encoded_size));
    }

    Ok((
        mp3_out_buffer,
        EncodeInfo {
            output_size: encoded_size,
            input_consumed: input.len(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_encoder_creation() {
        let result = encoder_mp3();
        assert!(result.is_ok(), "MP3 encoder should initialize successfully");
    }

    #[test_log::test]
    fn test_encode_mp3_basic() {
        let mut encoder = encoder_mp3().expect("Failed to create encoder");

        // Create a buffer of PCM samples
        let input: Vec<i16> = vec![0; 2048];

        let result = encode_mp3(&mut encoder, &input);

        assert!(result.is_ok(), "Encoding should succeed");
        let (output, info) = result.unwrap();

        assert!(!output.is_empty(), "Should produce output");
        assert_eq!(info.input_consumed, input.len(), "Should consume all input");
    }

    #[test_log::test]
    fn test_encode_mp3_non_zero_samples() {
        let mut encoder = encoder_mp3().expect("Failed to create encoder");

        // Create non-zero samples to ensure actual encoding happens
        let mut input: Vec<i16> = vec![0; 2048];
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        for (i, sample) in input.iter_mut().enumerate() {
            *sample = (i as i16 % 1000) - 500;
        }

        let result = encode_mp3(&mut encoder, &input);

        assert!(result.is_ok(), "Encoding should succeed");
        let (output, info) = result.unwrap();

        assert!(!output.is_empty(), "Should produce output");
        // Note: info.output_size may be 0 if all data is still buffered
        assert_eq!(info.input_consumed, input.len(), "Should consume all input");
    }

    #[test_log::test]
    fn test_encode_mp3_multiple_calls() {
        let mut encoder = encoder_mp3().expect("Failed to create encoder");

        // First encode
        let input1: Vec<i16> = vec![100; 2048];
        let result1 = encode_mp3(&mut encoder, &input1);
        assert!(result1.is_ok());
        let (_output1, info1) = result1.unwrap();
        assert_eq!(
            info1.input_consumed,
            input1.len(),
            "First encode should consume all input"
        );

        // Second encode
        let input2: Vec<i16> = vec![200; 2048];
        let result2 = encode_mp3(&mut encoder, &input2);
        assert!(result2.is_ok(), "Multiple encodes should work");
        let (_output2, info2) = result2.unwrap();
        assert_eq!(
            info2.input_consumed,
            input2.len(),
            "Second encode should consume all input"
        );
    }

    #[test_log::test]
    fn test_encode_mp3_empty_input() {
        let mut encoder = encoder_mp3().expect("Failed to create encoder");

        let input: Vec<i16> = vec![];
        let result = encode_mp3(&mut encoder, &input);

        // Empty input should succeed (may still produce output due to buffering/flushing)
        assert!(result.is_ok(), "Empty input should be handled");
        let (_output, info) = result.unwrap();
        assert_eq!(info.input_consumed, 0, "Should consume zero input");
    }

    #[test_log::test]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn test_encode_mp3_varying_samples() {
        let mut encoder = encoder_mp3().expect("Failed to create encoder");

        // Generate a more realistic audio pattern (stereo interleaved)
        let sample_count = 4096; // Should be larger than one MP3 frame
        let mut input: Vec<i16> = Vec::with_capacity(sample_count);

        for i in 0..sample_count / 2 {
            // Generate sine wave for left and right channels
            let t = i as f32 / 44100.0;
            let left = ((t * 440.0 * std::f32::consts::TAU).sin() * 16000.0) as i16;
            let right = ((t * 550.0 * std::f32::consts::TAU).sin() * 16000.0) as i16;
            input.push(left);
            input.push(right);
        }

        let result = encode_mp3(&mut encoder, &input);
        assert!(result.is_ok(), "Encoding varying samples should succeed");

        let (output, info) = result.unwrap();
        assert!(!output.is_empty(), "Should produce output");
        assert_eq!(info.input_consumed, input.len(), "Should consume all input");
    }
}
