//! Type conversion utilities for `tantivy` document values.
//!
//! This module provides implementations of the [`ToValueType`] trait for converting
//! values from Tantivy search engine documents into Rust types.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use tantivy::schema::{NamedFieldDocument, OwnedValue, Value as _};

use crate::{ParseError, ToValueType};

impl<'a> ToValueType<&'a str> for &'a OwnedValue {
    /// Converts a tantivy string value to a string slice.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the value is not a string
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_str()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }
}

impl<'a> ToValueType<&'a OwnedValue> for &'a OwnedValue {
    /// Returns the tantivy value as-is.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    fn to_value_type(self) -> Result<&'a OwnedValue, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a OwnedValue
where
    &'a OwnedValue: ToValueType<T>,
{
    /// Converts a tantivy value to an optional type.
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

// Numeric and string type conversions for tantivy `OwnedValue` references.
// Each implementation converts the tantivy value to the target Rust type.
// All return `ParseError::ConvertType` if the value is not a compatible type.

impl ToValueType<String> for &OwnedValue {
    fn to_value_type(self) -> Result<String, ParseError> {
        Ok(self
            .as_str()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_string())
    }
}

impl ToValueType<bool> for &OwnedValue {
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.as_bool()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))
    }
}

impl ToValueType<f32> for &OwnedValue {
    fn to_value_type(self) -> Result<f32, ParseError> {
        Ok(self
            .as_f64()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))? as f32)
    }
}

impl ToValueType<f64> for &OwnedValue {
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.as_f64()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))
    }
}

impl ToValueType<u8> for &OwnedValue {
    fn to_value_type(self) -> Result<u8, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))? as u8)
    }
}

impl ToValueType<u16> for &OwnedValue {
    fn to_value_type(self) -> Result<u16, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))? as u16)
    }
}

impl ToValueType<u32> for &OwnedValue {
    fn to_value_type(self) -> Result<u32, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))? as u32)
    }
}

impl ToValueType<u64> for &OwnedValue {
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.as_u64()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))
    }
}

/// Trait for extracting typed values from tantivy documents.
///
/// This trait provides methods to get values by field name from tantivy documents
/// and convert them to the desired Rust type.
pub trait ToValue<Type> {
    /// Extracts a value from a tantivy document field and converts it to type `T`.
    ///
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Type: 'a,
        &'a Type: ToValueType<T>;
}

impl ToValue<Vec<OwnedValue>> for NamedFieldDocument {
    /// Extracts values from a tantivy document field by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the field is missing
    /// * Returns [`ParseError`] if the values fail to convert to type `T`
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<OwnedValue>: 'a,
        &'a Vec<OwnedValue>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

impl ToValue<Vec<OwnedValue>> for &NamedFieldDocument {
    /// Extracts values from a tantivy document field by name.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::Parse`] if the field is missing
    /// * Returns [`ParseError`] if the values fail to convert to type `T`
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<OwnedValue>: 'a,
        &'a Vec<OwnedValue>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

/// Extracts values from a tantivy document field and converts them to type `T`.
///
/// # Errors
///
/// * If the value failed to parse
pub fn get_doc_value_types<'a, T>(
    value: &'a NamedFieldDocument,
    index: &str,
) -> Result<T, ParseError>
where
    &'a Vec<OwnedValue>: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index) {
        return inner.to_value_type();
    }

    Err(ParseError::Parse(format!("Missing value: '{index}'")))
}

/// Extracts the first value from a tantivy document field and converts it to type `T`.
///
/// # Errors
///
/// * If the value failed to parse
pub fn get_value_type<'a, T>(value: &'a NamedFieldDocument, index: &str) -> Result<T, ParseError>
where
    &'a OwnedValue: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index)
        && let Some(inner) = inner.first()
    {
        let inner = inner.to_value_type()?;
        return Ok(inner);
    }

    Err(ParseError::Parse(format!("Missing value: '{index}'")))
}

impl<'a> ToValueType<&'a Vec<OwnedValue>> for &'a Vec<OwnedValue> {
    /// Returns the vector of tantivy values as-is.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    fn to_value_type(self) -> Result<&'a Vec<OwnedValue>, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Vec<T>> for &'a Vec<OwnedValue>
where
    &'a OwnedValue: ToValueType<T>,
{
    /// Converts a vector of tantivy values to a vector of type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if any value fails to convert to type `T`
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.iter()
            .map(ToValueType::to_value_type)
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<'a, T> ToValueType<T> for &'a Vec<OwnedValue>
where
    &'a OwnedValue: ToValueType<T>,
{
    /// Converts the first tantivy value in the vector to type `T`.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError::ConvertType`] if the vector is empty
    /// * Returns [`ParseError`] if the value fails to convert to type `T`
    fn to_value_type(self) -> Result<T, ParseError> {
        self.first()
            .map(ToValueType::to_value_type)
            .ok_or_else(|| ParseError::ConvertType("&str".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_value_type_str() {
        let value = OwnedValue::Str("test".to_string());
        let result: Result<&str, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), "test");
    }

    #[test]
    fn test_to_value_type_string() {
        let value = OwnedValue::Str("test".to_string());
        let result: Result<String, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), "test");
    }

    #[test]
    fn test_to_value_type_bool() {
        let value = OwnedValue::Bool(true);
        let result: Result<bool, ParseError> = (&value).to_value_type();
        assert!(result.unwrap());

        let value = OwnedValue::Bool(false);
        let result: Result<bool, ParseError> = (&value).to_value_type();
        assert!(!result.unwrap());
    }

    #[test]
    fn test_to_value_type_u64() {
        let value = OwnedValue::U64(123);
        let result: Result<u64, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), 123);
    }

    #[test]
    fn test_to_value_type_f64() {
        let value = OwnedValue::F64(2.5);
        let result: Result<f64, ParseError> = (&value).to_value_type();
        assert!((result.unwrap() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_to_value_type_f32() {
        let value = OwnedValue::F64(2.5);
        let result: Result<f32, ParseError> = (&value).to_value_type();
        assert!((result.unwrap() - 2.5_f32).abs() < 0.001);
    }

    #[test]
    fn test_to_value_type_u8() {
        let value = OwnedValue::U64(42);
        let result: Result<u8, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_to_value_type_u16() {
        let value = OwnedValue::U64(1234);
        let result: Result<u16, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), 1234);
    }

    #[test]
    fn test_to_value_type_u32() {
        let value = OwnedValue::U64(12345);
        let result: Result<u32, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), 12345);
    }

    #[test]
    fn test_to_value_type_option() {
        let value = OwnedValue::Str("test".to_string());
        let result: Result<Option<String>, ParseError> = (&value).to_value_type();
        assert_eq!(result.unwrap(), Some("test".to_string()));
    }

    #[test]
    fn test_to_value_type_owned_value_identity() {
        let value = OwnedValue::U64(123);
        let result: Result<&OwnedValue, ParseError> = (&value).to_value_type();
        assert!(result.is_ok());
    }

    #[test]
    fn test_vec_owned_value_to_vec() {
        let values = vec![OwnedValue::U64(1), OwnedValue::U64(2), OwnedValue::U64(3)];
        let result: Result<Vec<u64>, ParseError> = (&values).to_value_type();
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_vec_owned_value_to_first_value() {
        let values = vec![OwnedValue::U64(42)];
        let result: Result<u64, ParseError> = (&values).to_value_type();
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_vec_owned_value_empty_error() {
        let values: Vec<OwnedValue> = vec![];
        let result: Result<u64, ParseError> = (&values).to_value_type();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
    }

    #[test]
    fn test_vec_owned_value_identity() {
        let values = vec![OwnedValue::U64(1), OwnedValue::U64(2)];
        let result: Result<&Vec<OwnedValue>, ParseError> = (&values).to_value_type();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_get_value_type_from_document() {
        use std::collections::BTreeMap;

        let mut fields = BTreeMap::new();
        fields.insert(
            "test_field".to_string(),
            vec![OwnedValue::Str("test_value".to_string())],
        );
        let doc = NamedFieldDocument(fields);

        let result: Result<String, ParseError> = get_value_type(&doc, "test_field");
        assert_eq!(result.unwrap(), "test_value");
    }

    #[test]
    fn test_get_value_type_missing_field() {
        use std::collections::BTreeMap;

        let fields = BTreeMap::new();
        let doc = NamedFieldDocument(fields);

        let result: Result<String, ParseError> = get_value_type(&doc, "missing_field");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::Parse(_)));
    }

    #[test]
    fn test_get_doc_value_types() {
        use std::collections::BTreeMap;

        let mut fields = BTreeMap::new();
        fields.insert(
            "numbers".to_string(),
            vec![OwnedValue::U64(1), OwnedValue::U64(2), OwnedValue::U64(3)],
        );
        let doc = NamedFieldDocument(fields);

        let result: Result<Vec<u64>, ParseError> = get_doc_value_types(&doc, "numbers");
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_to_value_trait_on_document() {
        use std::collections::BTreeMap;

        let mut fields = BTreeMap::new();
        fields.insert(
            "test".to_string(),
            vec![OwnedValue::Str("value".to_string())],
        );
        let doc = NamedFieldDocument(fields);

        let result: Result<String, ParseError> = doc.to_value("test");
        assert_eq!(result.unwrap(), "value");
    }

    #[test]
    fn test_to_value_trait_on_document_ref() {
        use std::collections::BTreeMap;

        let mut fields = BTreeMap::new();
        fields.insert("num".to_string(), vec![OwnedValue::U64(42)]);
        let doc = NamedFieldDocument(fields);

        let result: Result<u64, ParseError> = doc.to_value("num");
        assert_eq!(result.unwrap(), 42);
    }
}
