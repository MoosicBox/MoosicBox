use flacenc::{
    bitsink::{ByteSink, MemSink},
    component::BitRepr as _,
    error::{Verified, Verify as _},
    source::MemSource,
};
use thiserror::Error;

use crate::EncodeInfo;

pub struct Encoder {
    sink: ByteSink,
    pos: usize,
    encoder: Verified<flacenc::config::Encoder>,
}

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error(transparent)]
    Output(#[from] flacenc::error::OutputError<MemSink<u8>>),
    #[error(transparent)]
    Verify(#[from] flacenc::error::VerifyError),
    #[error("Encode error")]
    Encode(flacenc::error::EncodeError),
}

impl From<flacenc::error::EncodeError> for EncoderError {
    fn from(value: flacenc::error::EncodeError) -> Self {
        EncoderError::Encode(value)
    }
}

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
