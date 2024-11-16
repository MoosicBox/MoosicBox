#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "aac")]
pub mod aac;

#[cfg(feature = "flac")]
pub mod flac;

#[cfg(feature = "mp3")]
pub mod mp3;

#[cfg(feature = "opus")]
pub mod opus;

pub struct EncodeInfo {
    pub output_size: usize,
    pub input_consumed: usize,
}
