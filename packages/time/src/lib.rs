//! Time utilities with support for both standard system time and simulated time.
//!
//! This crate provides a unified interface for getting the current time, with the ability to
//! switch between real system time and simulated time for testing purposes. When the `simulator`
//! feature is enabled, time can be controlled programmatically for deterministic testing.
//!
//! # Features
//!
//! * `std` - Enables standard library time functions
//! * `simulator` - Enables time simulation capabilities for testing
//! * `chrono` - Adds support for `chrono` `DateTime` types
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(any(feature = "simulator", feature = "std"))]
//! # {
//! use switchy_time::now;
//! use std::time::SystemTime;
//!
//! let current_time = now();
//! # let _ = current_time; // Suppress unused variable warning
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "std")]
pub mod standard;

#[cfg(feature = "simulator")]
pub mod simulator;

#[allow(unused)]
macro_rules! impl_time {
    ($module:ident $(,)?) => {
        pub use $module::{instant_now, now};

        #[cfg(feature = "chrono")]
        pub use $module::{datetime_local_now, datetime_utc_now};
    };
}

#[cfg(feature = "simulator")]
impl_time!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_time!(standard);
