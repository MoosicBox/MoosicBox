//! Type conversion utilities for `serde_json` values.
//!
//! This module provides implementations of the [`ToValueType`] trait for converting
//! JSON values from the `serde_json` crate into Rust types. It includes support
//! for navigating nested JSON structures.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use serde_json::Value;

use crate::{ParseError, ToValueType};

/// Trait for navigating to nested values in JSON structures.
pub trait ToNested<Type> {
    /// Navigates to a nested value using a path of keys.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_nested<'a>(&'a self, path: &[&str]) -> Result<&'a Type, ParseError>;
}

impl ToNested<Value> for &Value {
    /// Navigates to a nested value using a path of keys.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if any key in the path is missing
    fn to_nested<'a>(&'a self, path: &[&str]) -> Result<&'a Value, ParseError> {
        get_nested_value(self, path)
    }
}

/// Navigates to a nested value in a JSON structure using a path of keys.
///
/// # Errors
///
/// * If the value failed to parse
pub fn get_nested_value<'a>(mut value: &'a Value, path: &[&str]) -> Result<&'a Value, ParseError> {
    for (i, x) in path.iter().enumerate() {
        if let Some(inner) = value.get(x) {
            value = inner;
            continue;
        }

        let message = if i > 0 {
            format!("Path '{}' missing value: '{}'", path[..i].join(" -> "), x)
        } else {
            format!("Missing value: '{x}' ({value})")
        };

        return Err(ParseError::Parse(message));
    }

    Ok(value)
}

impl<'a> ToValueType<&'a str> for &'a Value {
    /// Converts a JSON string value to a string slice.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_str()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }
}

impl<'a> ToValueType<&'a Value> for &'a Value {
    /// Returns the JSON value as-is.
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
    /// Converts a JSON value to an optional type.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if the value fails to convert to type `T`
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl<'a, T> ToValueType<Vec<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    /// Converts a JSON array to a vector of values.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an array
    /// * Returns [`ParseError`] if any array element fails to convert to type `T`
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.as_array()
            .ok_or_else(|| ParseError::ConvertType("Vec<T>".into()))?
            .iter()
            .map(ToValueType::to_value_type)
            .collect::<Result<Vec<_>, _>>()
    }
}

// Numeric and string type conversions for JSON `Value` references.
// Each implementation converts the JSON value to the target Rust type.
// All return `ParseError::ConvertType` if the value is not a compatible type.

impl ToValueType<String> for &Value {
    /// Converts a JSON value to a String.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<String, ParseError> {
        Ok(self
            .as_str()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_string())
    }
}

impl ToValueType<bool> for &Value {
    /// Converts a JSON value to a boolean.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a boolean
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.as_bool()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))
    }
}

impl ToValueType<f32> for &Value {
    /// Converts a JSON value to an f32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a number
    fn to_value_type(self) -> Result<f32, ParseError> {
        Ok(self
            .as_f64()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))? as f32)
    }
}

impl ToValueType<f64> for &Value {
    /// Converts a JSON value to an f64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a number
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.as_f64()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))
    }
}

impl ToValueType<u8> for &Value {
    /// Converts a JSON value to a u8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an unsigned integer
    fn to_value_type(self) -> Result<u8, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))? as u8)
    }
}

impl ToValueType<u16> for &Value {
    /// Converts a JSON value to a u16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an unsigned integer
    fn to_value_type(self) -> Result<u16, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))? as u16)
    }
}

impl ToValueType<u32> for &Value {
    /// Converts a JSON value to a u32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an unsigned integer
    fn to_value_type(self) -> Result<u32, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))? as u32)
    }
}

impl ToValueType<u64> for &Value {
    /// Converts a JSON value to a u64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an unsigned integer
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.as_u64()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))
    }
}

impl ToValueType<usize> for &Value {
    /// Converts a JSON value to a usize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not an unsigned integer
    fn to_value_type(self) -> Result<usize, ParseError> {
        self.as_u64()
            .map(|x| x as usize)
            .ok_or_else(|| ParseError::ConvertType("usize".into()))
    }
}

impl ToValueType<i8> for &Value {
    /// Converts a JSON value to an i8.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a signed integer
    fn to_value_type(self) -> Result<i8, ParseError> {
        Ok(self
            .as_i64()
            .ok_or_else(|| ParseError::ConvertType("i8".into()))? as i8)
    }
}

impl ToValueType<i16> for &Value {
    /// Converts a JSON value to an i16.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a signed integer
    fn to_value_type(self) -> Result<i16, ParseError> {
        Ok(self
            .as_i64()
            .ok_or_else(|| ParseError::ConvertType("i16".into()))? as i16)
    }
}

impl ToValueType<i32> for &Value {
    /// Converts a JSON value to an i32.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a signed integer
    fn to_value_type(self) -> Result<i32, ParseError> {
        Ok(self
            .as_i64()
            .ok_or_else(|| ParseError::ConvertType("i32".into()))? as i32)
    }
}

impl ToValueType<i64> for &Value {
    /// Converts a JSON value to an i64.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a signed integer
    fn to_value_type(self) -> Result<i64, ParseError> {
        self.as_i64()
            .ok_or_else(|| ParseError::ConvertType("i64".into()))
    }
}

impl ToValueType<isize> for &Value {
    /// Converts a JSON value to an isize.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a signed integer
    fn to_value_type(self) -> Result<isize, ParseError> {
        self.as_i64()
            .map(|x| x as isize)
            .ok_or_else(|| ParseError::ConvertType("isize".into()))
    }
}

/// Trait for extracting typed values from JSON by key.
pub trait ToValue {
    /// Extracts a value from a JSON object by key and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>;
}

impl ToValue for Value {
    /// Extracts a value from a JSON object by key.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the key is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Self: ToValueType<T>,
    {
        self.to_nested_value(&[index])
    }
}

impl ToValue for &Value {
    /// Extracts a value from a JSON object by key.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the key is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        self.to_nested_value(&[index])
    }
}

/// Trait for extracting typed values from nested JSON structures.
pub trait ToNestedValue {
    /// Navigates to a nested JSON value using a path and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>;
}

impl ToNestedValue for Value {
    /// Navigates to a nested JSON value and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if any key in the path is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Self: ToValueType<T>,
    {
        get_nested_value_type::<T>(self, path)
    }
}

impl ToNestedValue for &Value {
    /// Navigates to a nested JSON value and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if any key in the path is missing
    /// * Returns [`ParseError::ConvertType`] if the value fails to convert to type `T`
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        get_nested_value_type::<T>(self, path)
    }
}

/// Navigates to a nested value in a JSON structure and converts it to type `T`.
///
/// # Errors
///
/// * If the value failed to parse
pub fn get_nested_value_type<'a, T>(value: &'a Value, path: &[&str]) -> Result<T, ParseError>
where
    &'a Value: ToValueType<T>,
{
    let mut inner_value = value;

    for (i, x) in path.iter().enumerate() {
        if let Some(inner) = inner_value.get(x) {
            inner_value = inner;
            continue;
        }

        let message = if i > 0 {
            format!("Path '{}' missing value: '{x}'", path[..i].join(" -> "))
        } else {
            format!("Missing value: '{x}' ({value})")
        };

        return inner_value.missing_value(ParseError::Parse(message));
    }

    if inner_value.is_null() {
        return inner_value.missing_value(ParseError::ConvertType(format!(
            "{} found null",
            path.join(" -> "),
        )));
    }

    match inner_value.to_value_type() {
        Ok(inner) => Ok(inner),
        Err(err) => match err {
            ParseError::ConvertType(_) => Err(ParseError::ConvertType(
                if log::log_enabled!(log::Level::Debug) {
                    format!(
                        "Path '{}' failed to convert value to type: '{err:?}' ({})",
                        serde_json::to_string(value).unwrap_or_default(),
                        path.join(" -> "),
                    )
                } else {
                    format!(
                        "Path '{}' failed to convert value to type: '{err:?}'",
                        path.join(" -> "),
                    )
                },
            )),
            _ => Err(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_nested_value_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64": 123,
            },
        });

        assert_eq!(
            json.to_nested_value::<u64>(&["outer", "inner_u64"])
                .unwrap(),
            123_u64
        );
    }

    #[test]
    fn test_to_value_option_null_string() {
        let json = &serde_json::json!({
            "str": serde_json::Value::Null,
        });

        assert_eq!(json.to_value::<Option<String>>("str").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_string() {
        let json = &serde_json::json!({
            "str": "hey there",
            "u64": 123u64,
        });

        assert_eq!(
            json.to_value::<Option<String>>("str").unwrap(),
            Some("hey there".to_string())
        );

        assert_eq!(json.to_value::<Option<String>>("str2").unwrap(), None);

        assert_eq!(
            json.to_value::<Option<String>>("u64").err(),
            Some(ParseError::ConvertType(
                "Path 'u64' failed to convert value to type: 'ConvertType(\"String\")'".into()
            )),
        );

        let result: Option<String> = json.to_value("str").unwrap();
        assert_eq!(result, Some("hey there".to_string()));

        let result: Option<String> = json.to_value("str2").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_to_nested_value_option_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64": 123,
                "inner_str": "hey there",
            },
        });

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "inner_u64"])
                .unwrap(),
            Some(123_u64)
        );

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "bob"])
                .unwrap(),
            None
        );

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "inner_str"])
                .err(),
            Some(ParseError::ConvertType(
                "Path 'outer -> inner_str' failed to convert value to type: 'ConvertType(\"u64\")'"
                    .into()
            )),
        );
    }

    #[test]
    fn test_to_nested_value_vec_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64_array": [123, 124, 125],
            },
        });

        assert_eq!(
            json.to_nested_value::<Vec<u64>>(&["outer", "inner_u64_array"])
                .unwrap(),
            vec![123_u64, 124_u64, 125_u64]
        );
    }

    #[test]
    fn test_to_value_nested_vec_u64() {
        let json = &serde_json::json!({
            "items": [
                {"item": 123},
                {"item": 124},
                {"item": 125},
            ],
        });

        let values = json.to_value::<Vec<&Value>>("items").unwrap();
        let numbers = values
            .into_iter()
            .map(|value| value.to_value::<u64>("item").unwrap())
            .collect::<Vec<_>>();

        assert_eq!(numbers, vec![123_u64, 124_u64, 125_u64]);
    }
}
