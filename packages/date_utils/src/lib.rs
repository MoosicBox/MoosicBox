//! Date and time utilities for `MoosicBox`.
//!
//! This crate provides utility functions for parsing and working with dates and times.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "chrono")]
pub mod chrono;
