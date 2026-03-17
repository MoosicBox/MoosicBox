//! Logging utilities and macros for `MoosicBox` services.
//!
//! This crate provides optional integration with `free_log_client` for structured
//! logging initialization, as well as convenience macros and re-exports for
//! runtime log emission.

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
