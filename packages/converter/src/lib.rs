#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("OpusEncoder error")]
    OpusEncoder(opus::Error),
}

pub struct EncodeInfo {
    pub output_size: usize,
    pub input_consumed: usize,
}

impl From<opus::Error> for EncoderError {
    fn from(value: opus::Error) -> Self {
        EncoderError::OpusEncoder(value)
    }
}

pub fn encode_opus_float(input: &[f32], output: &mut [u8]) -> Result<EncodeInfo, EncoderError> {
    let mut encoder =
        opus::Encoder::new(48000, opus::Channels::Stereo, opus::Application::Audio).unwrap();

    let len = encoder.encode_float(input, output).unwrap();

    Ok(EncodeInfo {
        output_size: len,
        input_consumed: input.len(),
    })
}
