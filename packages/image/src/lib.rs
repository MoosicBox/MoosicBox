#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "libvips")]
pub mod libvips;

pub enum Encoding {
    Jpeg,
    Webp,
}
