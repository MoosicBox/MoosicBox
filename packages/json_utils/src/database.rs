use chrono::NaiveDateTime;
use moosicbox_database::{DatabaseValue, Row};
use thiserror::Error;

use crate::{MissingValue, ParseError, ToValueType};

#[derive(Debug, Error)]
pub enum DatabaseFetchError {
    #[error(transparent)]
    Database(#[from] moosicbox_database::DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl<'a> ToValueType<&'a str> for &'a DatabaseValue {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match self {
            DatabaseValue::String(ref str) => Ok(str),
            _ => Err(ParseError::ConvertType("&str".into())),
        }
    }
}

impl<'a> ToValueType<&'a DatabaseValue> for &'a DatabaseValue {
    fn to_value_type(self) -> Result<&'a DatabaseValue, ParseError> {
        Ok(self)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a DatabaseValue
where
    &'a DatabaseValue: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            DatabaseValue::Null => Ok(None),
            DatabaseValue::BoolOpt(None) => Ok(None),
            DatabaseValue::StringOpt(None) => Ok(None),
            DatabaseValue::NumberOpt(None) => Ok(None),
            DatabaseValue::UNumberOpt(None) => Ok(None),
            DatabaseValue::RealOpt(None) => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for &DatabaseValue {
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            DatabaseValue::String(ref str) => Ok(str.to_string()),
            DatabaseValue::DateTime(ref datetime) => {
                Ok(datetime.and_utc().to_rfc3339().to_string())
            }
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for &DatabaseValue {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for &DatabaseValue {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            DatabaseValue::Real(num) => Ok(*num as f32),
            _ => Err(ParseError::ConvertType("f32".into())),
        }
    }
}

impl ToValueType<f64> for &DatabaseValue {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            DatabaseValue::Real(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for &DatabaseValue {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as i8),
            DatabaseValue::UNumber(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for &DatabaseValue {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as i16),
            DatabaseValue::UNumber(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for &DatabaseValue {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as i32),
            DatabaseValue::UNumber(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for &DatabaseValue {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num),
            DatabaseValue::UNumber(num) => Ok(*num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for &DatabaseValue {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as isize),
            DatabaseValue::UNumber(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for &DatabaseValue {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as u8),
            DatabaseValue::UNumber(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for &DatabaseValue {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as u16),
            DatabaseValue::UNumber(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for &DatabaseValue {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as u32),
            DatabaseValue::UNumber(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for &DatabaseValue {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as u64),
            DatabaseValue::UNumber(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for &DatabaseValue {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num as usize),
            DatabaseValue::UNumber(num) => Ok(*num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl<'a, T> ToValueType<Vec<T>> for Vec<&'a Row>
where
    &'a Row: ToValueType<T>,
{
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
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.iter()
            .map(|row| row.to_value_type())
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<'a, T> ToValueType<Option<T>> for Option<&'a Row>
where
    &'a Row: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(|row| row.to_value_type()).transpose()
    }
}

impl<T> ToValueType<Option<T>> for Option<Row>
where
    Row: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(|row| row.to_value_type()).transpose()
    }
}

impl<'a, Type> MissingValue<Option<Type>> for &'a Row
where
    &'a Row: MissingValue<Type>,
{
    fn missing_value(&self, _error: ParseError) -> Result<Option<Type>, ParseError> {
        Ok(None)
    }
}

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

pub trait ToValue<Type> {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Type: ToValueType<T>,
        for<'a> &'a Row: MissingValue<T>;

    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}

impl ToValue<DatabaseValue> for DatabaseValue {
    fn to_value<T>(self, _index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Row: MissingValue<T>,
    {
        self.to_value_type()
    }
}

impl ToValue<DatabaseValue> for &Row {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Row: MissingValue<T>,
    {
        get_value_type(self, index)
    }
}

impl ToValue<DatabaseValue> for Row {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Row: MissingValue<T>,
    {
        get_value_type(&self, index)
    }
}

trait Get {
    fn get(&self, index: &str) -> Option<DatabaseValue>;
}

impl Get for &Row {
    fn get(&self, index: &str) -> Option<DatabaseValue> {
        moosicbox_database::Row::get(self, index)
    }
}

impl Get for Row {
    fn get(&self, index: &str) -> Option<DatabaseValue> {
        moosicbox_database::Row::get(self, index)
    }
}

fn get_value_type<T, X>(row: X, index: &str) -> Result<T, ParseError>
where
    DatabaseValue: ToValueType<T>,
    X: MissingValue<T> + Get + std::fmt::Debug,
{
    match row.get(index) {
        Some(inner) => match inner.to_value_type() {
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
        None => row.missing_value(ParseError::Parse(format!("Missing value: '{}'", index))),
    }
}

impl<T> ToValueType<Option<T>> for DatabaseValue
where
    DatabaseValue: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            DatabaseValue::Null => Ok(None),
            DatabaseValue::BoolOpt(None) => Ok(None),
            DatabaseValue::StringOpt(None) => Ok(None),
            DatabaseValue::NumberOpt(None) => Ok(None),
            DatabaseValue::UNumberOpt(None) => Ok(None),
            DatabaseValue::RealOpt(None) => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for DatabaseValue {
    fn to_value_type(self) -> Result<String, ParseError> {
        match self {
            DatabaseValue::String(ref str) => Ok(str.to_string()),
            DatabaseValue::DateTime(ref datetime) => {
                Ok(datetime.and_utc().to_rfc3339().to_string())
            }
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for DatabaseValue {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for DatabaseValue {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            DatabaseValue::Real(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<f64> for DatabaseValue {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            DatabaseValue::Real(num) => Ok(num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for DatabaseValue {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as i8),
            DatabaseValue::UNumber(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for DatabaseValue {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as i16),
            DatabaseValue::UNumber(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for DatabaseValue {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as i32),
            DatabaseValue::UNumber(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for DatabaseValue {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num),
            DatabaseValue::UNumber(num) => Ok(num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for DatabaseValue {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as isize),
            DatabaseValue::UNumber(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for DatabaseValue {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as u8),
            DatabaseValue::UNumber(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for DatabaseValue {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as u16),
            DatabaseValue::UNumber(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for DatabaseValue {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as u32),
            DatabaseValue::UNumber(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for DatabaseValue {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as u64),
            DatabaseValue::UNumber(num) => Ok(num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for DatabaseValue {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(num as usize),
            DatabaseValue::UNumber(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl ToValueType<NaiveDateTime> for DatabaseValue {
    fn to_value_type(self) -> Result<NaiveDateTime, ParseError> {
        match self {
            DatabaseValue::DateTime(value) => Ok(value),
            _ => Err(ParseError::ConvertType("NaiveDateTime".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_value_type_u64() {
        let value = &DatabaseValue::UNumber(123);

        assert_eq!(ToValueType::<u64>::to_value_type(value).unwrap(), 123_u64);
    }

    #[test]
    fn test_to_value_option_string_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UNumber(123))],
        };
        assert_eq!(row.to_value::<Option<String>>("bob").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_u64_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UNumber(123))],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_u64_where_property_exists_but_is_null() {
        let row = Row {
            columns: vec![("bob".to_string(), DatabaseValue::Null)],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_u64_where_property_exists_but_is_null_bool() {
        let row = Row {
            columns: vec![("bob".to_string(), DatabaseValue::BoolOpt(None))],
        };
        assert_eq!(row.to_value::<Option<u64>>("bob").unwrap(), None);
    }

    #[test]
    fn test_to_value_type_option_u64() {
        let value = &DatabaseValue::UNumber(123);
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
}
