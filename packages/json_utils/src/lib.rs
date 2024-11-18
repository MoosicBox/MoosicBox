#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("Failed to parse property: {0:?}")]
    Parse(String),
    #[error("Failed to convert to type: {0:?}")]
    ConvertType(String),
    #[error("Failed to convert to type: {0:?}")]
    MissingValue(String),
}

pub trait ToValueType<T> {
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value_type(self) -> Result<T, ParseError>;

    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<T, ParseError> {
        Err(error)
    }
}

pub trait MissingValue<Type> {
    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}
