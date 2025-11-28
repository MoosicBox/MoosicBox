//! Type conversion utilities for `rusqlite` values.
//!
//! This module provides implementations of the [`ToValueType`] trait for converting
//! `SQLite` values from the `rusqlite` crate into Rust types.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use rusqlite::{Row, types::Value};

use crate::{MissingValue, ParseError, ToValueType};

impl<'a> ToValueType<&'a str> for &'a Value {
    /// Converts a `SQLite` string value to a string slice.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match &self {
            Value::Text(x) => Ok(x),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }
}

impl<'a> ToValueType<&'a Value> for &'a Value {
    /// Returns the `SQLite` value as-is.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    fn to_value_type(self) -> Result<&'a Value, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    /// Converts a `SQLite` value to an optional type, returning `None` for null values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the non-null value fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            Value::Null => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

// Numeric and string type conversions for rusqlite `Value` references.
// Each implementation converts the `SQLite` value to the target Rust type.
// All return `ParseError::ConvertType` if the value is not a compatible type.

impl ToValueType<String> for &Value {
    /// Converts a `SQLite` value to a String.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            Value::Text(x) => Ok(x.clone()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for &Value {
    /// Converts a `SQLite` value to a boolean.
    ///
    /// Integers are converted where 1 = true and 0 = false.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for &Value {
    /// Converts a `SQLite` value to an f32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a real number
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Value::Real(num) => Ok(*num as f32),
            _ => Err(ParseError::ConvertType("f32".into())),
        }
    }
}

impl ToValueType<f64> for &Value {
    /// Converts a `SQLite` value to an f64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a real number
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Value::Real(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for &Value {
    /// Converts a `SQLite` value to an i8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for &Value {
    /// Converts a `SQLite` value to an i16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for &Value {
    /// Converts a `SQLite` value to an i32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for &Value {
    /// Converts a `SQLite` value to an i64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for &Value {
    /// Converts a `SQLite` value to an isize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for &Value {
    /// Converts a `SQLite` value to a u8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for &Value {
    /// Converts a `SQLite` value to a u16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for &Value {
    /// Converts a `SQLite` value to a u32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for &Value {
    /// Converts a `SQLite` value to a u64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u64),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for &Value {
    /// Converts a `SQLite` value to a usize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl<'a, Type> MissingValue<Option<Type>> for &'a Row<'a>
where
    &'a Row<'a>: MissingValue<Type>,
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
// These implementations allow the type system to work with rusqlite row conversions.

impl MissingValue<i8> for &Row<'_> {}
impl MissingValue<i16> for &Row<'_> {}
impl MissingValue<i32> for &Row<'_> {}
impl MissingValue<i64> for &Row<'_> {}
impl MissingValue<isize> for &Row<'_> {}
impl MissingValue<u8> for &Row<'_> {}
impl MissingValue<u16> for &Row<'_> {}
impl MissingValue<u32> for &Row<'_> {}
impl MissingValue<u64> for &Row<'_> {}
impl MissingValue<usize> for &Row<'_> {}
impl MissingValue<bool> for &Row<'_> {}
impl MissingValue<String> for &Row<'_> {}
impl MissingValue<&str> for &Row<'_> {}
impl MissingValue<f32> for &Row<'_> {}
impl MissingValue<f64> for &Row<'_> {}

/// Trait for extracting typed values from rusqlite database rows.
///
/// This trait provides methods to get values by column name from rusqlite rows
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
        for<'a> &'a Row<'a>: MissingValue<T>;

    /// Handles the case when a column value is missing.
    ///
    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value<T>(&self, error: ParseError) -> Result<T, ParseError> {
        Err(error)
    }
}

impl ToValue<Self> for Value {
    /// Converts the `SQLite` value directly to type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the value fails to convert to type `T`
    fn to_value<T>(self, _index: &str) -> Result<T, ParseError>
    where
        Self: ToValueType<T>,
        for<'a> &'a Row<'a>: MissingValue<T>,
    {
        self.to_value_type()
    }
}

impl ToValue<Value> for &Row<'_> {
    /// Extracts a value from a rusqlite row column by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the column is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Value: ToValueType<T>,
        for<'a> &'a Row<'a>: MissingValue<T>,
    {
        get_value_type(&self, index)
    }
}

impl ToValue<Value> for Row<'_> {
    /// Extracts a value from a rusqlite row column by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the column is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Value: ToValueType<T>,
        for<'a> &'a Row<'a>: MissingValue<T>,
    {
        get_value_type(&&self, index)
    }
}

/// Internal trait for getting raw `SQLite` values from rows.
trait Get {
    /// Gets a `SQLite` value by column name.
    ///
    /// # Errors
    ///
    /// * Returns [`rusqlite::Error`] if the column doesn't exist or has the wrong type
    fn get(&self, index: &str) -> Result<Value, rusqlite::Error>;
}

impl Get for &Row<'_> {
    fn get(&self, index: &str) -> Result<Value, rusqlite::Error> {
        rusqlite::Row::get::<_, Value>(self, index)
    }
}

impl Get for Row<'_> {
    fn get(&self, index: &str) -> Result<Value, rusqlite::Error> {
        rusqlite::Row::get::<_, Value>(self, index)
    }
}

/// Helper function to extract and convert a `SQLite` value from a row.
///
/// # Errors
///
/// * Returns [`ParseError::Parse`] if the column is missing
/// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
fn get_value_type<T, X>(row: &X, index: &str) -> Result<T, ParseError>
where
    Value: ToValueType<T>,
    X: MissingValue<T> + Get + std::fmt::Debug,
{
    match row.get(index) {
        Ok(inner) => match inner.to_value_type() {
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
        Err(err) => row.missing_value(ParseError::Parse(format!(
            "Missing value: '{index}' ({err:?})"
        ))),
    }
}

// Owned `Value` type conversions follow the same pattern as reference conversions.
// These implementations consume the value and convert it to the target type.

impl<T> ToValueType<Option<T>> for Value
where
    Self: ToValueType<T>,
{
    /// Converts an owned `SQLite` value to an optional type, returning `None` for null values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the non-null value fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            Self::Null => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for Value {
    /// Converts an owned `SQLite` value to a String.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<String, ParseError> {
        match &self {
            Self::Text(str) => Ok(str.clone()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for Value {
    /// Converts an owned `SQLite` value to a boolean.
    ///
    /// Integers are converted where 1 = true and 0 = false.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Self::Integer(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for Value {
    /// Converts an owned `SQLite` value to an f32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a real number
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Self::Real(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<f64> for Value {
    /// Converts an owned `SQLite` value to an f64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a real number
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Self::Real(num) => Ok(num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for Value {
    /// Converts an owned `SQLite` value to an i8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for Value {
    /// Converts an owned `SQLite` value to an i16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for Value {
    /// Converts an owned `SQLite` value to an i32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for Value {
    /// Converts an owned `SQLite` value to an i64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Self::Integer(num) => Ok(num),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for Value {
    /// Converts an owned `SQLite` value to an isize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for Value {
    /// Converts an owned `SQLite` value to a u8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for Value {
    /// Converts an owned `SQLite` value to a u16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for Value {
    /// Converts an owned `SQLite` value to a u32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for Value {
    /// Converts an owned `SQLite` value to a u64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as u64),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for Value {
    /// Converts an owned `SQLite` value to a usize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an integer
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Self::Integer(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_to_value_type_u64() {
        let value = &Value::Integer(123);

        assert_eq!(ToValueType::<u64>::to_value_type(value).unwrap(), 123_u64);
    }

    #[test_log::test]
    fn test_to_value_type_option_u64() {
        let value = &Value::Integer(123);
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).unwrap(),
            Some(123_u64)
        );

        let value = &Value::Null;
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).unwrap(),
            None
        );

        let value = &Value::Text("testttt".into());
        assert_eq!(
            ToValueType::<Option<u64>>::to_value_type(value).err(),
            Some(ParseError::ConvertType("u64".into())),
        );
    }

    #[test_log::test]
    fn test_to_value_type_str_reference() {
        let value = Value::Text("hello".to_string());
        let result: Result<&str, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), "hello");

        // Error case: wrong type
        let value = &Value::Integer(42);
        let result: Result<&str, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_string() {
        let value = &Value::Text("world".to_string());
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "world");

        // Error case: wrong type
        let value = &Value::Integer(42);
        let result: Result<String, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_bool() {
        // SQLite stores booleans as integers: 1 = true, 0 = false
        let value = &Value::Integer(1);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        let value = &Value::Integer(0);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(!result.unwrap());

        // Error case: wrong type
        let value = &Value::Text("true".to_string());
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_float_conversions() {
        // f64 from Real
        let value = &Value::Real(1.23456);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 1.23456).abs() < f64::EPSILON);

        // f32 from Real (with truncation)
        let value = &Value::Real(2.5);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < 0.001);

        // Error cases
        let value = &Value::Integer(42);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));

        let result: Result<f32, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_signed_integer_conversions() {
        let value = &Value::Integer(-42);

        let result: Result<i8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), -42_i8);

        let result: Result<i16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), -42_i16);

        let result: Result<i32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), -42_i32);

        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), -42_i64);

        let result: Result<isize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), -42_isize);

        // Error case: wrong type
        let value = &Value::Text("42".to_string());
        let result: Result<i64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_to_value_type_unsigned_integer_conversions() {
        let value = &Value::Integer(255);

        let result: Result<u8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 255_u8);

        let result: Result<u16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 255_u16);

        let result: Result<u32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 255_u32);

        let result: Result<u64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 255_u64);

        let result: Result<usize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 255_usize);
    }

    #[test_log::test]
    fn test_to_value_type_value_identity() {
        // Test that Value can be converted to itself
        let value = Value::Integer(123);
        let result: Result<&Value, ParseError> = (&value).to_value_type();
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_owned_value_string_conversion() {
        let value = Value::Text("owned".to_string());
        let result: Result<String, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), "owned");

        let value = Value::Integer(42);
        let result: Result<String, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_value_bool_conversion() {
        let value = Value::Integer(1);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(result.unwrap());

        let value = Value::Integer(0);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(!result.unwrap());

        let value = Value::Real(1.0);
        let result: Result<bool, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_value_float_conversions() {
        let value = Value::Real(1.5);
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 1.5).abs() < f64::EPSILON);

        let value = Value::Real(2.5);
        let result: Result<f32, ParseError> = value.to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < 0.001);

        let value = Value::Text("1.5".to_string());
        let result: Result<f64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_value_integer_conversions() {
        let value = Value::Integer(100);
        let result: Result<i8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_i8);

        let value = Value::Integer(1000);
        let result: Result<i16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 1000_i16);

        let value = Value::Integer(100_000);
        let result: Result<i32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 100_000_i32);

        let value = Value::Integer(1_000_000_000);
        let result: Result<i64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 1_000_000_000_i64);

        let value = Value::Integer(12345);
        let result: Result<isize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 12345_isize);

        // Unsigned conversions
        let value = Value::Integer(200);
        let result: Result<u8, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 200_u8);

        let value = Value::Integer(60000);
        let result: Result<u16, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 60000_u16);

        let value = Value::Integer(4_000_000_000);
        let result: Result<u32, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 4_000_000_000_u32);

        let value = Value::Integer(9_000_000_000_000);
        let result: Result<u64, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 9_000_000_000_000_u64);

        let value = Value::Integer(98765);
        let result: Result<usize, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), 98765_usize);

        // Error case
        let value = Value::Text("123".to_string());
        let result: Result<i64, ParseError> = value.to_value_type();
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test_log::test]
    fn test_owned_value_option_null() {
        let value = Value::Null;
        let result: Result<Option<String>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);

        let value = Value::Null;
        let result: Result<Option<i64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), None);
    }

    #[test_log::test]
    fn test_owned_value_option_some() {
        let value = Value::Text("test".to_string());
        let result: Result<Option<String>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), Some("test".to_string()));

        let value = Value::Integer(42);
        let result: Result<Option<i64>, ParseError> = value.to_value_type();
        assert_eq!(result.unwrap(), Some(42_i64));
    }

    #[test_log::test]
    fn test_option_missing_value_returns_none() {
        // Test that missing_value for Option types returns Ok(None)
        let value = Value::Integer(42);
        let result = <&Value as ToValueType<Option<String>>>::missing_value(
            &&value,
            ParseError::Parse("test".to_string()),
        );
        assert_eq!(result.unwrap(), None);

        // Owned value version
        let result = <Value as ToValueType<Option<String>>>::missing_value(
            &value,
            ParseError::Parse("test".to_string()),
        );
        assert_eq!(result.unwrap(), None);
    }
}
