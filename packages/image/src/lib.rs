#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use strum_macros::{AsRefStr, Display, EnumString};

/// Image manipulation utilities using the `image` crate.
#[cfg(feature = "image")]
pub mod image;
/// Image manipulation utilities using `libvips`.
#[cfg(not(target_os = "windows"))]
#[cfg(feature = "libvips")]
pub mod libvips;

/// Image encoding format.
#[derive(Debug, Copy, Clone, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Encoding {
    /// JPEG encoding format.
    Jpeg,
    /// WebP encoding format.
    Webp,
}
