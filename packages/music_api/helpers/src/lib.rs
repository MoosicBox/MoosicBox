#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "auth")]
pub use auth::ApiAuth;

#[cfg(feature = "auth")]
pub mod auth;

#[cfg(feature = "scan")]
pub mod scan;
