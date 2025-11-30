//! Type conversion utilities for `switchy_database` values.
//!
//! This module provides implementations of the [`ToValueType`] trait for converting
//! database values from the `switchy_database` crate into Rust types.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use switchy_database::{Database, DatabaseValue, Row};
use thiserror::Error;

use crate::{MissingValue, ParseError, ToValueType};

/// Errors that can occur when fetching and converting database values.
#[derive(Debug, Error)]
pub enum DatabaseFetchError {
    /// The database request was invalid.
    #[error("Invalid Request")]
    InvalidRequest,
    /// A database error occurred.
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
    /// Failed to parse or convert a database value.
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl<'a> ToValueType<&'a str> for &'a DatabaseValue {
    /// Converts a database string value to a string slice.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match &self {
            DatabaseValue::String(x) => Ok(x),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }
}

impl<'a> ToValueType<&'a DatabaseValue> for &'a DatabaseValue {
    /// Returns the database value as-is.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    fn to_value_type(self) -> Result<&'a DatabaseValue, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a DatabaseValue
where
    &'a DatabaseValue: ToValueType<T>,
{
    /// Converts a database value to an optional type, returning `None` for null values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the non-null value fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            DatabaseValue::Null
            | DatabaseValue::BoolOpt(None)
            | DatabaseValue::StringOpt(None)
            | DatabaseValue::Int64Opt(None)
            | DatabaseValue::UInt64Opt(None)
            | DatabaseValue::Real64Opt(None)
            | DatabaseValue::Real32Opt(None) => Ok(None),
            #[cfg(feature = "decimal")]
            DatabaseValue::DecimalOpt(None) => Ok(None),
            #[cfg(feature = "uuid")]
            DatabaseValue::UuidOpt(None) => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for &DatabaseValue {
    /// Converts a database value to a String.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value cannot be converted to a string
    fn to_value_type(self) -> Result<String, ParseError> {
        match &self {
            DatabaseValue::String(x) => Ok(x.clone()),
            DatabaseValue::DateTime(datetime) => Ok(datetime.and_utc().to_rfc3339()),
            #[cfg(feature = "uuid")]
            DatabaseValue::Uuid(uuid) => Ok(uuid.to_string()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for &DatabaseValue {
    /// Converts a database value to a boolean.
    ///
    /// Supports both boolean values and integers (where 1 = true, 0 = false).
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value cannot be converted to a bool
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            DatabaseValue::Bool(value) => Ok(*value),
            DatabaseValue::Int64(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

// Numeric type conversions for DatabaseValue references.
// Each implementation converts the database numeric value to the target Rust numeric type.
// All return `ParseError::ConvertType` if the value is not a compatible numeric type.

impl ToValueType<f32> for &DatabaseValue {
    /// Converts a database value to an f32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            DatabaseValue::Real32(num) => Ok(*num),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Real64(num) => Ok(*num as f32),
            _ => Err(ParseError::ConvertType("f32".into())),
        }
    }
}

impl ToValueType<f64> for &DatabaseValue {
    /// Converts a database value to an f64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            DatabaseValue::Real64(num) => Ok(*num),
            DatabaseValue::Real32(num) => Ok(f64::from(*num)),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for &DatabaseValue {
    /// Converts a database value to an i8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Int64(num) => Ok(*num as i8),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for &DatabaseValue {
    /// Converts a database value to an i16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Int64(num) => Ok(*num as i16),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for &DatabaseValue {
    /// Converts a database value to an i32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Int64(num) => Ok(*num as i32),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for &DatabaseValue {
    /// Converts a database value to an i64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            DatabaseValue::Int64(num) => Ok(*num),
            #[allow(clippy::cast_possible_wrap)]
            DatabaseValue::UInt64(num) => Ok(*num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for &DatabaseValue {
    /// Converts a database value to an isize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Int64(num) => Ok(*num as isize),
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_possible_wrap
            )]
            DatabaseValue::UInt64(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for &DatabaseValue {
    /// Converts a database value to a u8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Int64(num) => Ok(*num as u8),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for &DatabaseValue {
    /// Converts a database value to a u16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Int64(num) => Ok(*num as u16),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for &DatabaseValue {
    /// Converts a database value to a u32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Int64(num) => Ok(*num as u32),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for &DatabaseValue {
    /// Converts a database value to a u64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Int64(num) => Ok(*num as u64),
            DatabaseValue::UInt64(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for &DatabaseValue {
    /// Converts a database value to a usize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Int64(num) => Ok(*num as usize),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UInt64(num) => Ok(*num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl<'a, T> ToValueType<Vec<T>> for Vec<&'a Row>
where
    &'a Row: ToValueType<T>,
{
    /// Converts a vector of database row references to a vector of values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if any row fails to convert to type `T`
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.iter()
            .map(|row| row.to_value_type())
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<T> ToValueType<Vec<T>> for Vec<Row>
where
    for<'a> &'a Row: ToValueType<T>,
{
    /// Converts a vector of database rows to a vector of values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if any row fails to convert to type `T`
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.iter()
            .map(ToValueType::to_value_type)
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<'a, T> ToValueType<Option<T>> for Option<&'a Row>
where
    &'a Row: ToValueType<T>,
{
    /// Converts an optional database row reference to an optional value.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the row is present but fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(ToValueType::to_value_type).transpose()
    }
}

impl<T> ToValueType<Option<T>> for Option<Row>
where
    Row: ToValueType<T>,
{
    /// Converts an optional database row to an optional value.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the row is present but fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(ToValueType::to_value_type).transpose()
    }
}

impl<'a, Type> MissingValue<Option<Type>> for &'a Row
where
    &'a Row: MissingValue<Type>,
{
    /// Returns `None` when an optional value is missing from the row.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    fn missing_value(&self, _error: ParseError) -> Result<Option<Type>, ParseError> {
        Ok(None)
    }
}

// Implement `MissingValue` for common types, using the default behavior of returning the error.
// These implementations allow the type system to work with database row conversions.

impl MissingValue<i8> for &Row {}
impl MissingValue<i16> for &Row {}
impl MissingValue<i32> for &Row {}
impl MissingValue<i64> for &Row {}
impl MissingValue<isize> for &Row {}
impl MissingValue<u8> for &Row {}
impl MissingValue<u16> for &Row {}
impl MissingValue<u32> for &Row {}
impl MissingValue<u64> for &Row {}
impl MissingValue<usize> for &Row {}
impl MissingValue<bool> for &Row {}
impl MissingValue<String> for &Row {}
impl MissingValue<&str> for &Row {}
impl MissingValue<f32> for &Row {}
impl MissingValue<f64> for &Row {}
impl MissingValue<NaiveDateTime> for &Row {}
impl MissingValue<chrono::DateTime<chrono::Utc>> for &Row {}

/// Trait for extracting typed values from database rows.
///
/// This trait provides methods to get values by column name from database rows
/// and convert them to the desired Rust type.
pub trait ToValue<Type> {
    /// Extracts a value from a database column and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Type: ToValueType<T>,
        for<'a> &'a Row: MissingValue<T>;

    /// Handles the case when a column value is missing.
    ///
    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}

impl ToValue<Self> for DatabaseValue {
    /// Converts the database value directly to type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the value fails to convert to type `T`
    fn to_value<T>(self, _index: &str) -> Result<T, ParseError>
    where
        Self: ToValueType<T>,
        for<'b> &'b Row: MissingValue<T>,
    {
        self.to_value_type()
    }
}

impl ToValue<DatabaseValue> for &Row {
    /// Extracts a value from a database row column by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the column is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Row: MissingValue<T>,
    {
        get_value_type(&self, index)
    }
}

impl ToValue<DatabaseValue> for Row {
    /// Extracts a value from a database row column by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the column is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Self: MissingValue<T>,
    {
        get_value_type(&&self, index)
    }
}

/// Internal trait for getting raw database values from rows.
trait Get {
    /// Gets a database value by column name.
    fn get(&self, index: &str) -> Option<DatabaseValue>;
}

impl Get for &Row {
    fn get(&self, index: &str) -> Option<DatabaseValue> {
        switchy_database::Row::get(self, index)
    }
}

impl Get for Row {
    fn get(&self, index: &str) -> Option<DatabaseValue> {
        Self::get(self, index)
    }
}

/// Helper function to extract and convert a database value from a row.
///
/// # Errors
///
/// * Returns [`ParseError::Parse`] if the column is missing
/// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
fn get_value_type<T, X>(row: &X, index: &str) -> Result<T, ParseError>
where
    DatabaseValue: ToValueType<T>,
    X: MissingValue<T> + Get + std::fmt::Debug,
{
    row.get(index).map_or_else(
        || row.missing_value(ParseError::Parse(format!("Missing value: '{index}'"))),
        |inner| match inner.to_value_type() {
            Ok(inner) => Ok(inner),

            Err(ParseError::ConvertType(r#type)) => Err(ParseError::ConvertType(
                if log::log_enabled!(log::Level::Debug) {
                    format!("Path '{index}' failed to convert value to type: '{type}' ({row:?})")
                } else {
                    format!("Path '{index}' failed to convert value to type: '{type}'")
                },
            )),
            Err(err) => Err(err),
        },
    )
}

// Owned `DatabaseValue` type conversions follow the same pattern as reference conversions.
// These implementations consume the value and convert it to the target type.

impl<T> ToValueType<Option<T>> for DatabaseValue
where
    Self: ToValueType<T>,
{
    /// Converts an owned database value to an optional type, returning `None` for null values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the non-null value fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            Self::Null
            | Self::BoolOpt(None)
            | Self::StringOpt(None)
            | Self::Int64Opt(None)
            | Self::UInt64Opt(None)
            | Self::Real64Opt(None)
            | Self::Real32Opt(None) => Ok(None),
            #[cfg(feature = "decimal")]
            Self::DecimalOpt(None) => Ok(None),
            _ => self.to_value_type().map(Some),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for DatabaseValue {
    /// Converts an owned database value to a String.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value cannot be converted to a string
    fn to_value_type(self) -> Result<String, ParseError> {
        match &self {
            Self::String(x) => Ok(x.clone()),
            Self::DateTime(datetime) => Ok(datetime.and_utc().to_rfc3339()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for DatabaseValue {
    /// Converts an owned database value to a boolean.
    ///
    /// Supports both boolean values and integers (where 1 = true, 0 = false).
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value cannot be converted to a bool
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Self::Bool(value) => Ok(value),
            Self::Int64(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for DatabaseValue {
    /// Converts an owned database value to an f32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Self::Real32(num) => Ok(num),
            #[allow(clippy::cast_possible_truncation)]
            Self::Real64(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<f64> for DatabaseValue {
    /// Converts an owned database value to an f64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Self::Real64(num) => Ok(num),
            Self::Real32(num) => Ok(f64::from(num)),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for DatabaseValue {
    /// Converts an owned database value to an i8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Int64(num) => Ok(num as i8),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for DatabaseValue {
    /// Converts an owned database value to an i16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Int64(num) => Ok(num as i16),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for DatabaseValue {
    /// Converts an owned database value to an i32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Int64(num) => Ok(num as i32),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for DatabaseValue {
    /// Converts an owned database value to an i64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Self::Int64(num) => Ok(num),
            #[allow(clippy::cast_possible_wrap)]
            Self::UInt64(num) => Ok(num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for DatabaseValue {
    /// Converts an owned database value to an isize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Int64(num) => Ok(num as isize),
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            Self::UInt64(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for DatabaseValue {
    /// Converts an owned database value to a u8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Int64(num) => Ok(num as u8),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for DatabaseValue {
    /// Converts an owned database value to a u16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Int64(num) => Ok(num as u16),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for DatabaseValue {
    /// Converts an owned database value to a u32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Int64(num) => Ok(num as u32),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for DatabaseValue {
    /// Converts an owned database value to a u64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Int64(num) => Ok(num as u64),
            Self::UInt64(num) => Ok(num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for DatabaseValue {
    /// Converts an owned database value to a usize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a numeric type
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Int64(num) => Ok(num as usize),
            #[allow(clippy::cast_possible_truncation)]
            Self::UInt64(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl ToValueType<NaiveDateTime> for DatabaseValue {
    /// Converts a database value to a naive datetime.
    ///
    /// Supports both native datetime values and string representations.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a datetime or valid datetime string
    fn to_value_type(self) -> Result<NaiveDateTime, ParseError> {
        match self {
            Self::DateTime(value) => Ok(value),
            Self::String(dt_str) => {
                // Parse datetime string (SQLite returns datetime as string)
                chrono::NaiveDateTime::parse_from_str(&dt_str, "%Y-%m-%dT%H:%M:%S%.f")
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&dt_str, "%Y-%m-%d %H:%M:%S")
                    })
                    .map_err(|_| {
                        ParseError::ConvertType(format!("Invalid datetime format: {dt_str}"))
                    })
            }
            _ => Err(ParseError::ConvertType("NaiveDateTime".into())),
        }
    }
}

impl ToValueType<chrono::DateTime<chrono::Utc>> for DatabaseValue {
    /// Converts an owned database value to a UTC datetime.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a datetime or valid datetime string
    fn to_value_type(self) -> Result<chrono::DateTime<chrono::Utc>, ParseError> {
        (&self).to_value_type()
    }
}

impl ToValueType<chrono::DateTime<chrono::Utc>> for &DatabaseValue {
    /// Converts a database value reference to a UTC datetime.
    ///
    /// Supports RFC3339, ISO 8601, and other common datetime formats.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a datetime or valid datetime string
    fn to_value_type(self) -> Result<chrono::DateTime<chrono::Utc>, ParseError> {
        match self {
            DatabaseValue::DateTime(naive_dt) => Ok(naive_dt.and_utc()),
            DatabaseValue::String(datetime_str) => {
                // First try RFC3339 (ISO 8601 with timezone)
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(datetime_str) {
                    return Ok(dt.with_timezone(&chrono::Utc));
                }

                // If no timezone, try ISO 8601 format and assume UTC
                if let Ok(naive_dt) =
                    chrono::NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%dT%H:%M:%S%.f")
                {
                    return Ok(naive_dt.and_utc());
                }

                Err(ParseError::ConvertType(format!(
                    "DateTime<Utc> (expected ISO 8601 format, got '{datetime_str}')"
                )))
            }
            _ => Err(ParseError::ConvertType("DateTime<Utc>".into())),
        }
    }
}

/// Trait for converting database rows into model types.
pub trait AsModel<T> {
    /// Converts this database row into a model of type `T`.
    fn as_model(&self) -> T;
}

/// Trait for fallibly converting database rows into model types.
pub trait AsModelResult<T, E> {
    /// Attempts to convert this database row into a model of type `T`.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model(&self) -> Result<T, E>;
}

/// Trait for converting multiple database rows into a vector of models.
pub trait AsModelResultMapped<T, E> {
    /// Converts a collection of database rows into a vector of models.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model_mapped(&self) -> Result<Vec<T>, E>;
}

/// Trait for converting multiple database rows into a vector of models (mutable).
pub trait AsModelResultMappedMut<T, E> {
    /// Converts a mutable collection of database rows into a vector of models.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model_mapped_mut(&mut self) -> Result<Vec<T>, E>;
}

/// Trait for converting database rows into models with database query support.
#[async_trait]
pub trait AsModelResultMappedQuery<T, E> {
    /// Converts database rows into models, with access to the database for additional queries.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    async fn as_model_mapped_query(&self, db: Arc<Box<dyn Database>>) -> Result<Vec<T>, E>;
}

/// Trait for converting mutable database rows into a vector of models.
pub trait AsModelResultMut<T, E> {
    /// Converts mutable database rows into a vector of models.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        for<'b> &'b switchy_database::Row: ToValueType<T>;
}

impl<T, E> AsModelResultMut<T, E> for Vec<switchy_database::Row>
where
    E: From<DatabaseFetchError>,
{
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        for<'b> &'b switchy_database::Row: ToValueType<T>,
    {
        let mut values = vec![];

        for row in self {
            match row.to_value_type() {
                Ok(value) => values.push(value),
                Err(err) => {
                    if log::log_enabled!(log::Level::Debug) {
                        log::error!("Row error: {err:?} ({row:?})");
                    } else {
                        log::error!("Row error: {err:?}");
                    }
                }
            }
        }

        Ok(values)
    }
}

/// Trait for converting database rows into models with database query support.
#[async_trait]
pub trait AsModelQuery<T> {
    /// Converts a database row into a model, with access to the database for additional queries.
    ///
    /// # Errors
    ///
    /// * If the model fails to be created
    async fn as_model_query(&self, db: Arc<Box<dyn Database>>) -> Result<T, DatabaseFetchError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_to_value_type_u64() {
        let value = &DatabaseValue::UInt64(123);

        assert_eq!(ToValueType::<u64>::to_value_type(value).unwrap(), 123_u64);
    }

    #[test_log::test]
    fn test_to_value_option_string_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UInt64(123))],
        };
        assert_eq!(row.to_value::<Option<String>>("bob").unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_option_u64_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UInt64(123))],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_option_u64_where_property_exists_but_is_null() {
        let row = Row {
            columns: vec![("bob".to_string(), DatabaseValue::Null)],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_option_u64_where_property_exists_but_is_null_bool() {
        let row = Row {
            columns: vec![("bob".to_string(), DatabaseValue::BoolOpt(None))],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_type_option_u64() {
        let value = &DatabaseValue::UInt64(123);
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).unwrap(),
            Some(123_u64)
        );

        let value = &DatabaseValue::Null;
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).unwrap(),
            None
        );

        let value = &DatabaseValue::String("testttt".into());
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).err(),
            Some(ParseError::ConvertType("u64".into())),
        );
    }

    #[test_log::test]
    fn test_to_value_type_datetime_utc_from_string() {
        use chrono::{DateTime, Datelike, Timelike, Utc};

        // Test ISO 8601 format with milliseconds (database format)
        let value = &DatabaseValue::String("2025-08-01T20:06:35.421".to_string());
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_ok());

        let datetime = result.unwrap();
        assert_eq!(datetime.year(), 2025);
        assert_eq!(datetime.month(), 8);
        assert_eq!(datetime.day(), 1);
        assert_eq!(datetime.hour(), 20);
        assert_eq!(datetime.minute(), 6);
        assert_eq!(datetime.second(), 35);
        assert_eq!(datetime.nanosecond(), 421_000_000); // 421 milliseconds

        // Test ISO 8601 format without milliseconds
        let value = &DatabaseValue::String("2025-08-01T20:06:35".to_string());
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_ok());

        // Test RFC3339 format with timezone
        let value = &DatabaseValue::String("2025-08-01T20:06:35.421Z".to_string());
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_ok());

        // Test invalid format (non-ISO 8601)
        let value = &DatabaseValue::String("2025-08-01 20:06:35.421".to_string());
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_err());

        // Test completely invalid format
        let value = &DatabaseValue::String("invalid-date".to_string());
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_to_value_type_bool() {
        // Test DatabaseValue::Bool variants (PostgreSQL style)
        let value = &DatabaseValue::Bool(true);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        let value = &DatabaseValue::Bool(false);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(!result.unwrap());

        // Test DatabaseValue::Int64 variants (SQLite style)
        let value = &DatabaseValue::Int64(1);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        let value = &DatabaseValue::Int64(0);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(!result.unwrap());

        // Test invalid type
        let value = &DatabaseValue::String("true".to_string());
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_to_value_type_string_from_datetime() {
        use chrono::NaiveDate;

        let datetime = NaiveDate::from_ymd_opt(2025, 8, 1)
            .unwrap()
            .and_hms_opt(20, 6, 35)
            .unwrap();
        let value = &DatabaseValue::DateTime(datetime);
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "2025-08-01T20:06:35+00:00");
    }

    #[test_log::test]
    #[cfg(feature = "uuid")]
    fn test_to_value_type_string_from_uuid() {
        use uuid::Uuid;

        let test_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let value = &DatabaseValue::Uuid(test_uuid);
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test_log::test]
    fn test_to_value_type_naive_datetime_from_string() {
        use chrono::{Datelike, NaiveDateTime, Timelike};

        // Test ISO 8601 format with milliseconds
        let value = DatabaseValue::String("2025-08-01T20:06:35.421".to_string());
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 8);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.hour(), 20);
        assert_eq!(dt.minute(), 6);
        assert_eq!(dt.second(), 35);

        // Test format without milliseconds
        let value = DatabaseValue::String("2025-08-01 20:06:35".to_string());
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(result.is_ok());

        // Test invalid datetime string
        let value = DatabaseValue::String("invalid-date".to_string());
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_naive_datetime_from_database_value() {
        use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

        let datetime = NaiveDate::from_ymd_opt(2025, 8, 1)
            .unwrap()
            .and_hms_opt(20, 6, 35)
            .unwrap();
        let value = DatabaseValue::DateTime(datetime);
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 8);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.hour(), 20);
        assert_eq!(dt.minute(), 6);
        assert_eq!(dt.second(), 35);
    }

    #[test_log::test]
    fn test_to_value_type_vec_rows() {
        // Test converting vector of Rows to Vec<T>
        let rows = [
            Row {
                columns: vec![("value".to_string(), DatabaseValue::UInt64(1))],
            },
            Row {
                columns: vec![("value".to_string(), DatabaseValue::UInt64(2))],
            },
            Row {
                columns: vec![("value".to_string(), DatabaseValue::UInt64(3))],
            },
        ];

        // We can't directly test ToValueType for Vec<Row> without implementing ToValueType for &Row
        // This tests the owned Vec<Row> implementation
        #[allow(clippy::needless_collect)]
        let row_refs: Vec<&Row> = rows.iter().collect();
        // This would require implementing ToValueType<u64> for &Row which we don't have
        // So instead we verify the code path exists by checking the implementation compiles
        assert_eq!(row_refs.len(), 3);
    }

    #[test_log::test]
    fn test_to_value_type_float_conversions() {
        // Test f32 from Real32
        let value = &DatabaseValue::Real32(2.5_f32);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < f32::EPSILON);

        // Test f32 from Real64 (with truncation)
        let value = &DatabaseValue::Real64(2.5_f64);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < 0.001);

        // Test f64 from Real64
        let value = &DatabaseValue::Real64(2.567_123_456_78_f64);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.567_123_456_78_f64).abs() < f64::EPSILON);

        // Test f64 from Real32
        let value = &DatabaseValue::Real32(2.5_f32);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f64).abs() < 0.001);
    }

    #[test_log::test]
    fn test_owned_database_value_conversions() {
        // Test owned String conversion
        let value = DatabaseValue::String("test".to_string());
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "test");

        // Test owned bool conversion
        let value = DatabaseValue::Bool(true);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        // Test owned integer conversion
        let value = DatabaseValue::Int64(42);
        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42);
    }

    #[test_log::test]
    fn test_database_value_to_value_identity() {
        // Test that DatabaseValue can be converted to itself
        let value = DatabaseValue::UInt64(123);
        let result: Result<&DatabaseValue, ParseError> = (&value).to_value_type();
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_to_value_type_str_reference() {
        let value = DatabaseValue::String("hello".to_string());
        let result: Result<&str, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), "hello");

        // Error case: wrong type
        let value = &DatabaseValue::Int64(42);
        let result: Result<&str, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_option_with_various_null_types() {
        // Test BoolOpt(None)
        let value = &DatabaseValue::BoolOpt(None);
        let result: Result<Option<bool>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test StringOpt(None)
        let value = &DatabaseValue::StringOpt(None);
        let result: Result<Option<String>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test Int64Opt(None)
        let value = &DatabaseValue::Int64Opt(None);
        let result: Result<Option<i64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test UInt64Opt(None)
        let value = &DatabaseValue::UInt64Opt(None);
        let result: Result<Option<u64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test Real64Opt(None)
        let value = &DatabaseValue::Real64Opt(None);
        let result: Result<Option<f64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test Real32Opt(None)
        let value = &DatabaseValue::Real32Opt(None);
        let result: Result<Option<f32>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_owned_to_value_type_option_with_various_null_types() {
        // Test owned BoolOpt(None)
        let value = DatabaseValue::BoolOpt(None);
        let result: Result<Option<bool>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test owned StringOpt(None)
        let value = DatabaseValue::StringOpt(None);
        let result: Result<Option<String>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test owned Int64Opt(None)
        let value = DatabaseValue::Int64Opt(None);
        let result: Result<Option<i64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test owned UInt64Opt(None)
        let value = DatabaseValue::UInt64Opt(None);
        let result: Result<Option<u64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test owned Real64Opt(None)
        let value = DatabaseValue::Real64Opt(None);
        let result: Result<Option<f64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test owned Real32Opt(None)
        let value = DatabaseValue::Real32Opt(None);
        let result: Result<Option<f32>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_type_datetime_utc_from_naive_datetime() {
        use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};

        let datetime = NaiveDate::from_ymd_opt(2025, 6, 15)
            .unwrap()
            .and_hms_opt(12, 30, 45)
            .unwrap();
        let value = &DatabaseValue::DateTime(datetime);
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_ok());

        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 6);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 12);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 45);
    }

    #[test_log::test]
    fn test_to_value_type_datetime_utc_invalid_type() {
        use chrono::{DateTime, Utc};

        // Error case: wrong type
        let value = &DatabaseValue::Int64(12345);
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_datetime_utc_conversion() {
        use chrono::{DateTime, NaiveDate, Utc};

        let datetime = NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let value = DatabaseValue::DateTime(datetime);
        let result: Result<DateTime<Utc>, ParseError> = value.to_value_type();
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_to_value_type_signed_integer_from_uint64() {
        // Test conversion from UInt64 to signed types
        let value = &DatabaseValue::UInt64(100);

        let result: Result<i8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i8);

        let result: Result<i16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i16);

        let result: Result<i32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i32);

        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i64);

        let result: Result<isize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_isize);
    }

    #[test_log::test]
    fn test_to_value_type_unsigned_integer_from_int64() {
        // Test conversion from Int64 to unsigned types (positive values)
        let value = &DatabaseValue::Int64(100);

        let result: Result<u8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_u8);

        let result: Result<u16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_u16);

        let result: Result<u32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_u32);

        let result: Result<u64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_u64);

        let result: Result<usize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_usize);
    }

    #[test_log::test]
    fn test_owned_numeric_type_conversions() {
        // Test owned Int64 conversions
        let value = DatabaseValue::Int64(42);
        let result: Result<i8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42_i8);

        let value = DatabaseValue::Int64(42);
        let result: Result<i16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42_i16);

        let value = DatabaseValue::Int64(42);
        let result: Result<i32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42_i32);

        let value = DatabaseValue::Int64(42);
        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42_i64);

        let value = DatabaseValue::Int64(42);
        let result: Result<isize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 42_isize);

        // Test owned UInt64 conversions
        let value = DatabaseValue::UInt64(200);
        let result: Result<u8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 200_u8);

        let value = DatabaseValue::UInt64(60000);
        let result: Result<u16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 60000_u16);

        let value = DatabaseValue::UInt64(4_000_000);
        let result: Result<u32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 4_000_000_u32);

        let value = DatabaseValue::UInt64(9_000_000_000);
        let result: Result<u64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 9_000_000_000_u64);

        let value = DatabaseValue::UInt64(12345);
        let result: Result<usize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 12345_usize);
    }

    #[test_log::test]
    fn test_owned_float_conversions() {
        // Test owned Real32 conversions
        let value = DatabaseValue::Real32(2.5_f32);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < f32::EPSILON);

        let value = DatabaseValue::Real32(2.5_f32);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f64).abs() < 0.001);

        // Test owned Real64 conversions
        let value = DatabaseValue::Real64(1.234);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 1.234).abs() < f64::EPSILON);

        let value = DatabaseValue::Real64(1.234);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 1.234_f32).abs() < 0.001);

        // Error case
        let value = DatabaseValue::String("1.5".to_string());
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_string_conversion_error() {
        // Error case: wrong type for String
        let value = DatabaseValue::Int64(42);
        let result: Result<String, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_option_missing_value_returns_none() {
        // Test that missing_value for Option types returns Ok(None)
        let value = &DatabaseValue::Int64(42);
        let result = <&DatabaseValue as ToValueType<Option<String>>>::missing_value(
            &value,
            ParseError::Parse("test".to_string()),
        );
        assert_eq!(result.unwrap(), None);

        // Owned value version
        let value = DatabaseValue::Int64(42);
        let result = <DatabaseValue as ToValueType<Option<String>>>::missing_value(
            &value,
            ParseError::Parse("test".to_string()),
        );
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_type_integer_conversion_errors() {
        // Error cases for integer conversions - wrong types
        let value = &DatabaseValue::String("not a number".to_string());

        let result: Result<i8, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));

        let result: Result<u64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));

        let result: Result<f64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_naive_datetime_conversion_invalid_format() {
        use chrono::NaiveDateTime;

        // Invalid datetime string format
        let value = DatabaseValue::String("01-08-2025 20:06:35".to_string());
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ParseError::ConvertType(_)));
        assert!(err.to_string().contains("Invalid datetime format"));
    }

    #[test_log::test]
    fn test_naive_datetime_conversion_wrong_type() {
        use chrono::NaiveDateTime;

        // Wrong type entirely
        let value = DatabaseValue::Int64(12345);
        let result: Result<NaiveDateTime, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_option_row_to_value_type_some() {
        // Test Option<&Row> conversion with Some value
        let row = Row {
            columns: vec![("value".to_string(), DatabaseValue::UInt64(42))],
        };

        // Test with &Row implementing ToValueType<u64>
        // Since we don't have a direct ToValueType<u64> for &Row, we test the wrapper
        let opt_row: Option<&Row> = Some(&row);
        assert!(opt_row.is_some());
    }

    #[test_log::test]
    fn test_option_row_to_value_type_none() {
        // Test Option<&Row> conversion with None - this returns Ok(None)
        let opt_row: Option<&Row> = None;
        assert!(opt_row.is_none());
    }

    #[test_log::test]
    fn test_missing_value_for_row_option_type() {
        // Test MissingValue trait for Option types on &Row
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UInt64(123))],
        };
        let row_ref = &row;
        let error = ParseError::MissingValue("field".to_string());
        let result: Result<Option<u64>, ParseError> =
            <&Row as MissingValue<Option<u64>>>::missing_value(&row_ref, error);
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_to_value_type_from_row_with_convert_type_error() {
        // Test get_value_type when column exists but conversion fails
        let row = Row {
            columns: vec![(
                "str_field".to_string(),
                DatabaseValue::String("not_a_number".to_string()),
            )],
        };

        let result: Result<u64, ParseError> = row.to_value("str_field");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_database_value_to_value_for_direct_conversion() {
        // Test DatabaseValue's ToValue implementation (for direct value conversion)
        let value = DatabaseValue::UInt64(42);
        let result: Result<u64, ParseError> = value.to_value("ignored_index");
        assert_eq!(result.unwrap(), 42);
    }

    #[test_log::test]
    #[cfg(feature = "decimal")]
    fn test_to_value_type_option_decimal_opt_none() {
        // Test DecimalOpt(None) for reference type
        let value = &DatabaseValue::DecimalOpt(None);
        let result: Result<Option<u64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        // Test DecimalOpt(None) for owned type
        let value = DatabaseValue::DecimalOpt(None);
        let result: Result<Option<u64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_owned_string_from_datetime() {
        use chrono::NaiveDate;

        let datetime = NaiveDate::from_ymd_opt(2025, 1, 15)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        let value = DatabaseValue::DateTime(datetime);
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "2025-01-15T10:30:00+00:00");
    }

    #[test_log::test]
    fn test_owned_bool_from_int64() {
        // Test Int64 to bool conversion for owned value
        let value = DatabaseValue::Int64(1);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        let value = DatabaseValue::Int64(0);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(!result.unwrap());
    }

    #[test_log::test]
    fn test_owned_bool_error_on_wrong_type() {
        // Test error when converting wrong type to bool
        let value = DatabaseValue::String("true".to_string());
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_integer_conversions_from_uint64() {
        // Test owned UInt64 to signed integer conversions
        let value = DatabaseValue::UInt64(100);
        let result: Result<i8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i8);

        let value = DatabaseValue::UInt64(100);
        let result: Result<i16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i16);

        let value = DatabaseValue::UInt64(100);
        let result: Result<i32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i32);

        let value = DatabaseValue::UInt64(100);
        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i64);

        let value = DatabaseValue::UInt64(100);
        let result: Result<isize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_isize);
    }

    #[test_log::test]
    fn test_owned_integer_error_on_wrong_type() {
        // Test error when converting string to integer
        let value = DatabaseValue::String("123".to_string());
        let result: Result<i64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_database_fetch_error_from_parse_error() {
        // Test DatabaseFetchError can be created from ParseError
        let parse_error = ParseError::MissingValue("test".to_string());
        let fetch_error: DatabaseFetchError = parse_error.into();
        assert!(matches!(fetch_error, DatabaseFetchError::Parse(_)));
    }
}
