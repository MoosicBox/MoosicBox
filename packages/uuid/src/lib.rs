//! UUID generation utilities with optional deterministic simulation mode.
//!
//! This crate provides a unified interface for generating UUIDs that can switch
//! between truly random generation (via the `uuid` feature) and deterministic
//! generation (via the `simulator` feature) for testing and simulation purposes.
//!
//! # Features
//!
//! * `uuid` - Use the standard `uuid` crate for random UUID generation
//! * `simulator` - Use deterministic UUID generation with a configurable seed
//!
//! # Examples
//!
//! ```
//! // Generate a UUID (random or deterministic based on feature flags)
//! let id = switchy_uuid::new_v4();
//! let id_string = switchy_uuid::new_v4_string();
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "uuid")]
pub mod uuid;

#[cfg(feature = "simulator")]
pub mod simulator;

#[allow(unused)]
macro_rules! impl_uuid {
    ($module:ident $(,)?) => {
        pub use $module::{new_v4, new_v4_string};
    };
}

#[cfg(feature = "simulator")]
impl_uuid!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "uuid"))]
impl_uuid!(uuid);
