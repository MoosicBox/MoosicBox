//! Audio encoding utilities for multiple formats.
//!
//! This crate provides encoding functionality for various audio formats including AAC, FLAC,
//! MP3, and Opus. Each format is available through a feature flag and provides encoder
//! initialization and encoding functions.
//!
//! # Features
//!
//! * `aac` - AAC encoding support using fdk-aac
//! * `flac` - FLAC encoding support using flacenc
//! * `mp3` - MP3 encoding support using mp3lame-encoder
//! * `opus` - Opus encoding support with Ogg container
//!
//! All features are enabled by default.
//!
//! # Example
//!
//! ```rust
//! # #[cfg(feature = "aac")]
//! # {
//! # use moosicbox_audio_encoder::aac::{encoder_aac, encode_aac};
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let encoder = encoder_aac()?;
//! let input: Vec<i16> = vec![0; 2048];
//! let mut output = vec![0u8; 8192];
//! let info = encode_aac(&encoder, &input, &mut output)?;
//! println!("Encoded {} samples into {} bytes", info.input_consumed, info.output_size);
//! # Ok(())
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "aac")]
pub mod aac;

#[cfg(feature = "flac")]
pub mod flac;

#[cfg(feature = "mp3")]
pub mod mp3;

#[cfg(feature = "opus")]
pub mod opus;

/// Information about an encoding operation.
///
/// Contains the amount of output produced and input consumed during encoding.
pub struct EncodeInfo {
    /// Number of bytes written to the output buffer
    pub output_size: usize,
    /// Number of input samples consumed from the input buffer
    pub input_consumed: usize,
}
