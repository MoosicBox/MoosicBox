#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "v1")]
pub mod v1;

#[cfg(all(feature = "v1", not(feature = "v2")))]
pub use v1::*;

#[cfg(any(feature = "v2", not(feature = "v1")))]
pub mod v2;

#[cfg(any(feature = "v2", not(feature = "v1")))]
pub use v2::*;

pub mod font_metrics;
pub mod layout;

pub use eframe;
