#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

mod constants;
mod decoder;
mod pvq;

pub use decoder::CeltDecoder;
