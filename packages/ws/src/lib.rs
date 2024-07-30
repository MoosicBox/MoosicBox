#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "ws")]
mod ws;

#[cfg(feature = "ws")]
pub use ws::*;

pub mod models;
