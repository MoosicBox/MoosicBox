#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_audio_decoder::AudioDecode;
use symphonia::core::audio::{AudioBuffer, Signal as _};
use symphonia::core::conv::{FromSample, IntoSample as _};
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;

pub mod encoders;

#[cfg(all(
    not(windows),
    any(feature = "pulseaudio-standard", feature = "pulseaudio-simple")
))]
pub mod pulseaudio;

#[cfg(feature = "cpal")]
pub mod cpal;

pub trait AudioWrite {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError>;
    fn flush(&mut self) -> Result<(), AudioOutputError>;
}

impl AudioDecode for Box<dyn AudioWrite> {
    fn decoded(
        &mut self,
        decoded: symphonia::core::audio::AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), moosicbox_audio_decoder::AudioDecodeError> {
        self.write(decoded)
            .map_err(|_e| moosicbox_audio_decoder::AudioDecodeError::PlayStream)?;
        Ok(())
    }
}

impl AudioDecode for &mut dyn AudioWrite {
    fn decoded(
        &mut self,
        decoded: symphonia::core::audio::AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), moosicbox_audio_decoder::AudioDecodeError> {
        self.write(decoded)
            .map_err(|_e| moosicbox_audio_decoder::AudioDecodeError::PlayStream)?;
        Ok(())
    }
}

impl From<Box<dyn AudioWrite>> for Box<dyn AudioDecode> {
    fn from(value: Box<dyn AudioWrite>) -> Self {
        Box::new(value)
    }
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioOutputError {
    #[error("OpenStreamError")]
    OpenStream,
    #[error("PlayStreamError")]
    PlayStream,
    #[error("StreamClosedError")]
    StreamClosed,
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[allow(unused)]
fn to_samples<S: FromSample<f32> + Default + Clone>(decoded: AudioBuffer<f32>) -> Vec<S> {
    let n_channels = decoded.spec().channels.count();
    let n_samples = decoded.frames() * n_channels;
    let mut buf: Vec<S> = vec![S::default(); n_samples];

    // Interleave the source buffer channels into the sample buffer.
    for ch in 0..n_channels {
        let ch_slice = decoded.chan(ch);

        for (dst, decoded) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
            *dst = (*decoded).into_sample();
        }
    }

    buf
}
