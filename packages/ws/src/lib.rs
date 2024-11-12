#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "ws")]
mod ws;

#[cfg(feature = "ws")]
pub use ws::*;

pub mod models;
