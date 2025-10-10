#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod constants;
mod decoder;
mod pvq;

pub use constants::CELT_NUM_BANDS;
pub use decoder::CeltDecoder;
