#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "free_log")]
mod free_log;

#[cfg(feature = "free_log")]
pub use free_log::*;

#[cfg(feature = "macros")]
mod macros;

#[cfg(feature = "macros")]
pub use macros::*;
