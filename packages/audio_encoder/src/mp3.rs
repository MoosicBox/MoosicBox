#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::EncodeInfo;

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Encoder error")]
    Encoder(mp3lame_encoder::EncodeError),
    #[error("Encoder error")]
    Id3Tag(mp3lame_encoder::Id3TagError),
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
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
    }

    let encoded_size = encoder.flush::<FlushNoGap>(mp3_out_buffer.spare_capacity_mut())?;
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
    }

    Ok((
        mp3_out_buffer,
        EncodeInfo {
            output_size: encoded_size,
            input_consumed: input.len(),
        },
    ))
}
