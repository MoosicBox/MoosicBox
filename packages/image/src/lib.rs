#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use strum_macros::{AsRefStr, Display, EnumString};

#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "libvips")]
pub mod libvips;

#[derive(Debug, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Encoding {
    Jpeg,
    Webp,
}
