#![allow(clippy::module_name_repetitions)]

use bytes::Bytes;
use symphonia::core::audio::{AudioBuffer, SignalSpec};

use crate::AudioOutputError;

#[cfg(feature = "aac")]
pub mod aac;
#[cfg(feature = "flac")]
pub mod flac;
#[cfg(feature = "mp3")]
pub mod mp3;
#[cfg(feature = "opus")]
pub mod opus;

pub trait AudioEncoder: Send + Sync {
    /// # Errors
    ///
    /// * If the audio fails to encode
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError>;
    fn spec(&self) -> SignalSpec;
}
