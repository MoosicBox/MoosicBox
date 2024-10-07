#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "free_log")]
mod free_log;

#[cfg(feature = "free_log")]
pub use free_log::*;

#[cfg(feature = "macros")]
mod macros;

#[cfg(feature = "macros")]
pub use macros::*;
