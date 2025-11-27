//! Arbitrary value generators for property-based testing.
//!
//! This crate provides [`proptest::arbitrary::Arbitrary`] implementations for various
//! domain-specific types used in testing, including CSS identifiers, XML strings,
//! and JSON-compatible values.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "css")]
pub mod css;

#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "serde")]
pub mod serde;
