use rusqlite::{types::Value, Row};

use crate::{ParseError, ToValueType};

impl<'a> ToValueType<&'a str> for &'a Value {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match self {
            Value::Text(ref str) => Ok(str),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
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
        match self {
            Value::Text(ref str) => Ok(str.to_string()),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<String, ParseError> {
        Err(error)
    }
}

impl ToValueType<bool> for &Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<bool, ParseError> {
        Err(error)
    }
}

impl ToValueType<f32> for &Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Value::Real(num) => Ok(*num as f32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<f32, ParseError> {
        Err(error)
    }
}

impl ToValueType<f64> for &Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Value::Real(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<f64, ParseError> {
        Err(error)
    }
}

impl ToValueType<i8> for &Value {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i8, ParseError> {
        Err(error)
    }
}

impl ToValueType<i16> for &Value {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i16, ParseError> {
        Err(error)
    }
}

impl ToValueType<i32> for &Value {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i32, ParseError> {
        Err(error)
    }
}

impl ToValueType<i64> for &Value {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i64, ParseError> {
        Err(error)
    }
}

impl ToValueType<isize> for &Value {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<isize, ParseError> {
        Err(error)
    }
}

impl ToValueType<u8> for &Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u8, ParseError> {
        Err(error)
    }
}

impl ToValueType<u16> for &Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u16, ParseError> {
        Err(error)
    }
}

impl ToValueType<u32> for &Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u32, ParseError> {
        Err(error)
    }
}

impl ToValueType<u64> for &Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as u64),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u64, ParseError> {
        Err(error)
    }
}

impl ToValueType<usize> for &Value {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Value::Integer(num) => Ok(*num as usize),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<usize, ParseError> {
        Err(error)
    }
}

pub trait ToValue<Type> {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Type: ToValueType<T>;
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
    if let Ok(inner) = row.get::<_, Value>(index) {
        return inner.to_value_type();
    }

    Err(ParseError::Parse(format!("Missing value: '{}'", index)))
}

impl<T> ToValueType<Option<T>> for Value
where
    Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }

    fn missing_value(self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            Value::Text(ref str) => Ok(str.to_string()),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<String, ParseError> {
        Err(error)
    }
}

impl ToValueType<bool> for Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Value::Integer(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<bool, ParseError> {
        Err(error)
    }
}

impl ToValueType<f32> for Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            Value::Real(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<f32, ParseError> {
        Err(error)
    }
}

impl ToValueType<f64> for Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Value::Real(num) => Ok(num),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<f64, ParseError> {
        Err(error)
    }
}

impl ToValueType<i8> for Value {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i8, ParseError> {
        Err(error)
    }
}

impl ToValueType<i16> for Value {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i16, ParseError> {
        Err(error)
    }
}

impl ToValueType<i32> for Value {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i32, ParseError> {
        Err(error)
    }
}

impl ToValueType<i64> for Value {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Value::Integer(num) => Ok(num),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<i64, ParseError> {
        Err(error)
    }
}

impl ToValueType<isize> for Value {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<isize, ParseError> {
        Err(error)
    }
}

impl ToValueType<u8> for Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u8, ParseError> {
        Err(error)
    }
}

impl ToValueType<u16> for Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u16, ParseError> {
        Err(error)
    }
}

impl ToValueType<u32> for Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u32, ParseError> {
        Err(error)
    }
}

impl ToValueType<u64> for Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as u64),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<u64, ParseError> {
        Err(error)
    }
}

impl ToValueType<usize> for Value {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            Value::Integer(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }

    fn missing_value(self, error: ParseError) -> Result<usize, ParseError> {
        Err(error)
    }
}
