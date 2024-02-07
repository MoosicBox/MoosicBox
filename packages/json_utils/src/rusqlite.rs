use rusqlite::{types::Value, Row};

use crate::{ParseError, ToValueType};

impl<'a> ToValueType<&'a str> for &'a Value {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match self {
            Value::Text(ref str) => Ok(str),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }
}

impl<'a> ToValueType<&'a Value> for &'a Value {
    fn to_value_type(self) -> Result<&'a Value, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
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

impl ToValueType<String> for &Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            Value::Text(ref str) => Ok(str.to_string()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for &Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for &Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Value::Real(num) => Ok(*num as f32),
            _ => Err(ParseError::ConvertType("f32".into())),
        }
    }
}

impl ToValueType<f64> for &Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Value::Real(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for &Value {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for &Value {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for &Value {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for &Value {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for &Value {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for &Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for &Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for &Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for &Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u64),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for &Value {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

pub trait ToValue<Type> {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Type: ToValueType<T>;

    fn missing_value<T>(&self, error: ParseError) -> Result<T, ParseError> {
        Err(error)
    }
}

impl ToValue<Value> for Value {
    fn to_value<T>(self, _index: &str) -> Result<T, ParseError>
    where
        Value: ToValueType<T>,
    {
        self.to_value_type()
    }
}

impl ToValue<Value> for &Row<'_> {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Value: ToValueType<T>,
    {
        get_value_type(self, index)
    }
}

impl ToValue<Value> for Row<'_> {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Value: ToValueType<T>,
    {
        get_value_type(&self, index)
    }
}

pub fn get_value_type<T>(row: &Row<'_>, index: &str) -> Result<T, ParseError>
where
    Value: ToValueType<T>,
{
    match row.get::<_, Value>(index) {
        Ok(inner) => match inner.to_value_type() {
            Ok(inner) => Ok(inner),

            Err(ParseError::ConvertType(r#type)) => Err(ParseError::ConvertType(
                if log::log_enabled!(log::Level::Debug) {
                    format!(
                        "Path '{}' failed to convert value to type: '{}' ({row:?})",
                        index, r#type,
                    )
                } else {
                    format!(
                        "Path '{}' failed to convert value to type: '{}'",
                        index, r#type,
                    )
                },
            )),
            Err(err) => Err(err),
        },
        Err(err) => row.missing_value(ParseError::Parse(format!(
            "Missing value: '{}' ({err:?})",
            index
        ))),
    }
}

impl<T> ToValueType<Option<T>> for Value
where
    Value: ToValueType<T>,
{
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

impl ToValueType<String> for Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            Value::Text(ref str) => Ok(str.to_string()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Value::Integer(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Value::Real(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<f64> for Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Value::Real(num) => Ok(num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for Value {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for Value {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for Value {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for Value {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Value::Integer(num) => Ok(num),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for Value {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u64),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for Value {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_value_type_u64() {
        let value = &Value::Integer(123);

        assert_eq!(ToValueType::<u64>::to_value_type(value).unwrap(), 123_u64);
    }

    #[test]
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
}
