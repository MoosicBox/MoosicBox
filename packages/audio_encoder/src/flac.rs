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
    sink: ByteSink,
    pos: usize,
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
