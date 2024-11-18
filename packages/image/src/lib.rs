#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use strum_macros::{AsRefStr, Display, EnumString};

#[cfg(feature = "image")]
pub mod image;
#[cfg(not(target_os = "windows"))]
#[cfg(feature = "libvips")]
pub mod libvips;

#[derive(Debug, Copy, Clone, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Encoding {
    Jpeg,
    Webp,
}
