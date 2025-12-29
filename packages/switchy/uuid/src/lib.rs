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
//! * `serde` - Enable serde serialization/deserialization support
//!
//! # Examples
//!
//! ```
//! use switchy_uuid::Uuid;
//!
//! // Generate a UUID (random or deterministic based on feature flags)
//! # #[cfg(any(feature = "uuid", feature = "simulator"))]
//! # {
//! let id = switchy_uuid::new_v4();
//! let id_string = switchy_uuid::new_v4_string();
//!
//! // Parse a UUID from string
//! let parsed: Uuid = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
//!
//! // Use in data structures
//! assert!(!id.is_nil());
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod wrapper;

pub use wrapper::{ParseError, Uuid};

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

impl Uuid {
    /// Generates a new random UUID v4.
    ///
    /// This is a convenience method that calls the appropriate backend
    /// based on feature flags (simulator or uuid crate).
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::new_v4();
    /// assert_eq!(uuid.get_version_num(), 4);
    /// ```
    #[cfg(any(feature = "simulator", feature = "uuid"))]
    #[must_use]
    pub fn new_v4() -> Self {
        new_v4()
    }
}
