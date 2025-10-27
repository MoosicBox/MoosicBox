//! Audio encoders for compressing decoded audio into various formats.
//!
//! This module provides the [`AudioEncoder`] trait and implementations for encoding
//! decoded audio samples into compressed formats like AAC, FLAC, MP3, and Opus.
//! Each encoder is available through its corresponding feature flag.

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
    /// Encodes decoded audio samples into a compressed format.
    ///
    /// # Errors
    ///
    /// * If the audio fails to encode
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError>;

    /// Returns the audio signal specification for this encoder.
    fn spec(&self) -> SignalSpec;
}
