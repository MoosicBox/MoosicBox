use fdk_aac::enc::{BitRate, ChannelMode, Encoder, EncoderParams, Transport};
use thiserror::Error;

use crate::EncodeInfo;

#[derive(Debug, Error)]
pub enum EncoderError {
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
        EncoderError::Encoder(value)
    }
}

pub fn encoder_aac() -> Result<Encoder, EncoderError> {
    let encoder = Encoder::new(EncoderParams {
        bit_rate: BitRate::VbrVeryHigh,
        sample_rate: 48_000,
        transport: Transport::Adts,
        channels: ChannelMode::Stereo,
    })?;
    Ok(encoder)
}

pub fn encode_aac(
    encoder: &mut Encoder,
    input: &[i16],
    buf: &mut [u8],
) -> Result<EncodeInfo, EncoderError> {
    let info = encoder.encode(input, buf)?;

    Ok(info.into())
}
