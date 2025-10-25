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
