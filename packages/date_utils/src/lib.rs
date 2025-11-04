//! Date and time utilities for `MoosicBox`.
//!
//! This crate provides utility functions for parsing and working with dates and times.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Date/time parsing utilities using the `chrono` crate.
///
/// This module re-exports types from `chrono` and provides the [`parse_date_time`]
/// function for flexible date/time string parsing.
///
/// [`parse_date_time`]: self::chrono::parse_date_time
#[cfg(feature = "chrono")]
pub mod chrono;
