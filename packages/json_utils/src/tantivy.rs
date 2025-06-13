#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use tantivy::schema::{NamedFieldDocument, OwnedValue, Value as _};

use crate::{ParseError, ToValueType};

impl<'a> ToValueType<&'a str> for &'a OwnedValue {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_str()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }
}

impl<'a> ToValueType<&'a OwnedValue> for &'a OwnedValue {
    fn to_value_type(self) -> Result<&'a OwnedValue, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a OwnedValue
where
    &'a OwnedValue: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

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

pub trait ToValue<Type> {
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Type: 'a,
        &'a Type: ToValueType<T>;
}

impl ToValue<Vec<OwnedValue>> for NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<OwnedValue>: 'a,
        &'a Vec<OwnedValue>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

impl ToValue<Vec<OwnedValue>> for &NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<OwnedValue>: 'a,
        &'a Vec<OwnedValue>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

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

/// # Errors
///
/// * If the value failed to parse
pub fn get_value_type<'a, T>(value: &'a NamedFieldDocument, index: &str) -> Result<T, ParseError>
where
    &'a OwnedValue: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index) {
        if let Some(inner) = inner.first() {
            let inner = inner.to_value_type()?;
            return Ok(inner);
        }
    }

    Err(ParseError::Parse(format!("Missing value: '{index}'")))
}

impl<'a> ToValueType<&'a Vec<OwnedValue>> for &'a Vec<OwnedValue> {
    fn to_value_type(self) -> Result<&'a Vec<OwnedValue>, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Vec<T>> for &'a Vec<OwnedValue>
where
    &'a OwnedValue: ToValueType<T>,
{
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
    fn to_value_type(self) -> Result<T, ParseError> {
        self.first()
            .map(ToValueType::to_value_type)
            .ok_or_else(|| ParseError::ConvertType("&str".into()))?
    }
}
