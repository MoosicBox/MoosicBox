//! Arbitrary value generators for property-based testing.
//!
//! This crate provides [`proptest::arbitrary::Arbitrary`] implementations for various
//! domain-specific types used in testing, including CSS identifiers, XML strings,
//! and JSON-compatible values.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// CSS identifier generators for property-based testing.
///
/// This module provides [`css::CssIdentifierString`] for generating valid CSS identifiers.
#[cfg(feature = "css")]
pub mod css;

/// XML-compatible string generators for property-based testing.
///
/// This module provides [`xml::XmlString`] and [`xml::XmlAttrNameString`] for generating
/// XML-safe content, along with character validation utilities.
#[cfg(feature = "xml")]
pub mod xml;

/// JSON-compatible value generators for property-based testing.
///
/// This module provides [`serde::JsonValue`], [`serde::JsonF64`], and [`serde::JsonF32`]
/// for generating JSON-compatible values.
#[cfg(feature = "serde")]
pub mod serde;
