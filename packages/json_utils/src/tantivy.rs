use tantivy::schema::{NamedFieldDocument, Value};

use crate::ParseError;

pub trait ToValueType<T> {
    fn to_value_type(self) -> Result<T, ParseError>;
    fn missing_value(self, error: ParseError) -> Result<T, ParseError>;
}

impl<'a> ToValueType<&'a str> for &'a Value {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_text()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<&'a str, ParseError> {
        Err(error)
    }
}

impl<'a> ToValueType<&'a Value> for &'a Value {
    fn to_value_type(self) -> Result<&'a Value, ParseError> {
        Ok(self)
    }

    fn missing_value(self, error: ParseError) -> Result<&'a Value, ParseError> {
        Err(error)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }

    fn missing_value(self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for &Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        Ok(self
            .as_text()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_string())
    }

    fn missing_value(self, error: ParseError) -> Result<String, ParseError> {
        Err(error)
    }
}

impl ToValueType<bool> for &Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.as_bool()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<bool, ParseError> {
        Err(error)
    }
}

impl ToValueType<f32> for &Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        Ok(self
            .as_f64()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))? as f32)
    }

    fn missing_value(self, error: ParseError) -> Result<f32, ParseError> {
        Err(error)
    }
}

impl ToValueType<f64> for &Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.as_f64()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<f64, ParseError> {
        Err(error)
    }
}

impl ToValueType<u8> for &Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))? as u8)
    }

    fn missing_value(self, error: ParseError) -> Result<u8, ParseError> {
        Err(error)
    }
}

impl ToValueType<u16> for &Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))? as u16)
    }

    fn missing_value(self, error: ParseError) -> Result<u16, ParseError> {
        Err(error)
    }
}

impl ToValueType<u32> for &Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))? as u32)
    }

    fn missing_value(self, error: ParseError) -> Result<u32, ParseError> {
        Err(error)
    }
}

impl ToValueType<u64> for &Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.as_u64()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<u64, ParseError> {
        Err(error)
    }
}

pub trait ToValue {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>;
}

impl ToValue for NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        get_value_type(self, index)
    }
}

impl ToValue for &NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        get_value_type(self, index)
    }
}

pub fn get_value_types<'a, T>(
    value: &'a NamedFieldDocument,
    index: &str,
) -> Result<Vec<T>, ParseError>
where
    &'a Value: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index) {
        let inner = inner
            .iter()
            .map(|x| x.to_value_type())
            .collect::<Result<Vec<_>, _>>()?;

        return Ok(inner);
    }

    Err(ParseError::Parse(format!("Missing value: '{}'", index)))
}

pub fn get_value_type<'a, T>(value: &'a NamedFieldDocument, index: &str) -> Result<T, ParseError>
where
    &'a Value: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index) {
        if let Some(inner) = inner.first() {
            let inner = inner.to_value_type()?;
            return Ok(inner);
        }
    }

    Err(ParseError::Parse(format!("Missing value: '{}'", index)))
}
