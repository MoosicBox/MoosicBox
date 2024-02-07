#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

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

pub trait ToValueType<T>: MissingValue<T> {
    fn to_value_type(self) -> Result<T, ParseError>;
}

pub trait MissingValue<Type> {
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}
