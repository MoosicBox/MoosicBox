//! Utilities for converting JSON and database values to Rust types.
//!
//! This crate provides traits and error types for converting values from various sources
//! (JSON, database rows, etc.) into Rust types in a consistent way.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

#[cfg(feature = "database")]
pub mod database;

#[cfg(feature = "rusqlite")]
pub mod rusqlite;

#[cfg(feature = "serde_json")]
pub mod serde_json;

#[cfg(feature = "tantivy")]
pub mod tantivy;

/// Errors that can occur when parsing or converting values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Failed to parse a property from the source value.
    #[error("Failed to parse property: {0:?}")]
    Parse(String),
    /// Failed to convert the value to the target type.
    #[error("Failed to convert to type: {0:?}")]
    ConvertType(String),
    /// A required value was missing from the source.
    #[error("Missing required value: {0:?}")]
    MissingValue(String),
}

/// Trait for converting a value to a target type.
///
/// This trait is implemented by various source types (database values, JSON values, etc.)
/// to provide a uniform interface for type conversion.
pub trait ToValueType<T> {
    /// Converts this value to the target type.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value_type(self) -> Result<T, ParseError>;

    /// Handles conversion when the value is missing from the source.
    ///
    /// The default implementation returns the provided error, but implementations
    /// can override this to provide default values (e.g., `None` for `Option<T>`).
    ///
    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<T, ParseError> {
        Err(error)
    }
}

/// Trait for handling missing values during conversion.
///
/// This trait is implemented by source types (like database rows) to define behavior
/// when a requested field is missing.
pub trait MissingValue<Type> {
    /// Handles the case when a value is missing from the source.
    ///
    /// The default implementation returns the provided error.
    ///
    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_parse_error_display() {
        let err = ParseError::Parse("test property".to_string());
        assert_eq!(
            err.to_string(),
            "Failed to parse property: \"test property\""
        );

        let err = ParseError::ConvertType("u64".to_string());
        assert_eq!(err.to_string(), "Failed to convert to type: \"u64\"");

        let err = ParseError::MissingValue("field_name".to_string());
        assert_eq!(err.to_string(), "Missing required value: \"field_name\"");
    }

    #[test_log::test]
    fn test_parse_error_eq() {
        assert_eq!(
            ParseError::Parse("test".to_string()),
            ParseError::Parse("test".to_string())
        );
        assert_ne!(
            ParseError::Parse("test".to_string()),
            ParseError::Parse("other".to_string())
        );
        assert_ne!(
            ParseError::Parse("test".to_string()),
            ParseError::ConvertType("test".to_string())
        );
    }
}
