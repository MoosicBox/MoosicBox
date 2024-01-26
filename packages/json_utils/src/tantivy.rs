use tantivy::schema::{NamedFieldDocument, Value};

use crate::{MissingValue, ParseError};

pub trait ToValueType<T>: MissingValue<T> {
    fn to_value_type(self) -> Result<T, ParseError>;
}

impl<'a> MissingValue<&'a str> for &'a Value {}
impl<'a> ToValueType<&'a str> for &'a Value {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_text()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }
}

impl<'a> MissingValue<&'a Value> for &'a Value {}
impl<'a> ToValueType<&'a Value> for &'a Value {
    fn to_value_type(self) -> Result<&'a Value, ParseError> {
        Ok(self)
    }
}

impl<'a, T> MissingValue<Option<T>> for &'a Value {
    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}
impl<'a, T> ToValueType<Option<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }
}

impl MissingValue<String> for &Value {}
impl ToValueType<String> for &Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        Ok(self
            .as_text()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_string())
    }
}

impl MissingValue<bool> for &Value {}
impl ToValueType<bool> for &Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.as_bool()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))
    }
}

impl MissingValue<f32> for &Value {}
impl ToValueType<f32> for &Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        Ok(self
            .as_f64()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))? as f32)
    }
}

impl MissingValue<f64> for &Value {}
impl ToValueType<f64> for &Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.as_f64()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))
    }
}

impl MissingValue<u8> for &Value {}
impl ToValueType<u8> for &Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))? as u8)
    }
}

impl MissingValue<u16> for &Value {}
impl ToValueType<u16> for &Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))? as u16)
    }
}

impl MissingValue<u32> for &Value {}
impl ToValueType<u32> for &Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))? as u32)
    }
}

impl MissingValue<u64> for &Value {}
impl ToValueType<u64> for &Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.as_u64()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))
    }
}

pub trait ToValue<Type> {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Type: 'a,
        &'a Type: ToValueType<T>;
}

impl ToValue<Vec<Value>> for NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<Value>: 'a,
        &'a Vec<Value>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

impl ToValue<Vec<Value>> for &NamedFieldDocument {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        Vec<Value>: 'a,
        &'a Vec<Value>: ToValueType<T>,
    {
        get_doc_value_types(self, index)
    }
}

pub fn get_doc_value_types<'a, T>(
    value: &'a NamedFieldDocument,
    index: &str,
) -> Result<T, ParseError>
where
    &'a Vec<Value>: ToValueType<T>,
{
    if let Some(inner) = value.0.get(index) {
        return inner.to_value_type();
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

impl<'a> MissingValue<&'a str> for &'a Vec<Value> {}
impl<'a> ToValueType<&'a str> for &'a Vec<Value> {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))?
            .to_value_type()
    }
}

impl<'a> MissingValue<&'a Vec<Value>> for &'a Vec<Value> {}
impl<'a> ToValueType<&'a Vec<Value>> for &'a Vec<Value> {
    fn to_value_type(self) -> Result<&'a Vec<Value>, ParseError> {
        Ok(self)
    }
}

impl<'a, T> MissingValue<Vec<T>> for &'a Vec<Value> {}
impl<'a, T> ToValueType<Vec<T>> for &'a Vec<Value>
where
    &'a Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.iter()
            .map(|inner| inner.to_value_type())
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<'a, T> MissingValue<Option<T>> for &'a Vec<Value> {
    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}
impl<'a, T> ToValueType<Option<T>> for &'a Vec<Value>
where
    &'a Vec<Value>: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }
}

impl MissingValue<String> for &Vec<Value> {}
impl ToValueType<String> for &Vec<Value> {
    fn to_value_type(self) -> Result<String, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_value_type()
    }
}

impl MissingValue<bool> for &Vec<Value> {}
impl ToValueType<bool> for &Vec<Value> {
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))?
            .to_value_type()
    }
}

impl MissingValue<f32> for &Vec<Value> {}
impl ToValueType<f32> for &Vec<Value> {
    fn to_value_type(self) -> Result<f32, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))?
            .to_value_type()
    }
}

impl MissingValue<f64> for &Vec<Value> {}
impl ToValueType<f64> for &Vec<Value> {
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))?
            .to_value_type()
    }
}

impl MissingValue<u8> for &Vec<Value> {}
impl ToValueType<u8> for &Vec<Value> {
    fn to_value_type(self) -> Result<u8, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))?
            .to_value_type()
    }
}

impl MissingValue<u16> for &Vec<Value> {}
impl ToValueType<u16> for &Vec<Value> {
    fn to_value_type(self) -> Result<u16, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))?
            .to_value_type()
    }
}

impl MissingValue<u32> for &Vec<Value> {}
impl ToValueType<u32> for &Vec<Value> {
    fn to_value_type(self) -> Result<u32, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))?
            .to_value_type()
    }
}

impl MissingValue<u64> for &Vec<Value> {}
impl ToValueType<u64> for &Vec<Value> {
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.first()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))?
            .to_value_type()
    }
}
