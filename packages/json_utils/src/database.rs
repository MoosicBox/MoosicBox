use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use switchy_database::{Database, DatabaseValue, Row};
use thiserror::Error;

use crate::{MissingValue, ParseError, ToValueType};

#[derive(Debug, Error)]
pub enum DatabaseFetchError {
    #[error("Invalid Request")]
    InvalidRequest,
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl<'a> ToValueType<&'a str> for &'a DatabaseValue {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        match &self {
            DatabaseValue::String(x) => Ok(x),
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
            DatabaseValue::Null
            | DatabaseValue::BoolOpt(None)
            | DatabaseValue::StringOpt(None)
            | DatabaseValue::Int64Opt(None)
            | DatabaseValue::UInt64Opt(None)
            | DatabaseValue::Real64Opt(None)
            | DatabaseValue::Real32Opt(None) => Ok(None),
            _ => self.to_value_type().map(|inner| Some(inner)),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for &DatabaseValue {
    fn to_value_type(self) -> Result<String, ParseError> {
        match &self {
            DatabaseValue::String(x) => Ok(x.to_string()),
            DatabaseValue::DateTime(datetime) => Ok(datetime.and_utc().to_rfc3339()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for &DatabaseValue {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            DatabaseValue::Bool(value) => Ok(*value),
            DatabaseValue::Int64(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for &DatabaseValue {
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
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            DatabaseValue::Real64(num) => Ok(*num),
            DatabaseValue::Real32(num) => Ok(f64::from(*num)),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for &DatabaseValue {
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
            .map(ToValueType::to_value_type)
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<'a, T> ToValueType<Option<T>> for Option<&'a Row>
where
    &'a Row: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(ToValueType::to_value_type).transpose()
    }
}

impl<T> ToValueType<Option<T>> for Option<Row>
where
    Row: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.map(ToValueType::to_value_type).transpose()
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
impl MissingValue<chrono::DateTime<chrono::Utc>> for &Row {}

pub trait ToValue<Type> {
    /// # Errors
    ///
    /// * If the value failed to parse
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        Type: ToValueType<T>,
        for<'a> &'a Row: MissingValue<T>;

    /// # Errors
    ///
    /// * If the missing value failed to parse
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}

impl ToValue<Self> for DatabaseValue {
    fn to_value<T>(self, _index: &str) -> Result<T, ParseError>
    where
        Self: ToValueType<T>,
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
        get_value_type(&self, index)
    }
}

impl ToValue<DatabaseValue> for Row {
    fn to_value<T>(self, index: &str) -> Result<T, ParseError>
    where
        DatabaseValue: ToValueType<T>,
        for<'b> &'b Self: MissingValue<T>,
    {
        get_value_type(&&self, index)
    }
}

trait Get {
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

impl<T> ToValueType<Option<T>> for DatabaseValue
where
    Self: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        match self {
            Self::Null
            | Self::BoolOpt(None)
            | Self::StringOpt(None)
            | Self::Int64Opt(None)
            | Self::UInt64Opt(None)
            | Self::Real64Opt(None)
            | Self::Real32Opt(None) => Ok(None),
            _ => self.to_value_type().map(Some),
        }
    }

    fn missing_value(&self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl ToValueType<String> for DatabaseValue {
    fn to_value_type(self) -> Result<String, ParseError> {
        match &self {
            Self::String(x) => Ok(x.to_string()),
            Self::DateTime(datetime) => Ok(datetime.and_utc().to_rfc3339()),
            _ => Err(ParseError::ConvertType("String".into())),
        }
    }
}

impl ToValueType<bool> for DatabaseValue {
    fn to_value_type(self) -> Result<bool, ParseError> {
        match self {
            Self::Bool(value) => Ok(value),
            Self::Int64(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for DatabaseValue {
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
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Self::Real64(num) => Ok(num),
            Self::Real32(num) => Ok(f64::from(num)),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for DatabaseValue {
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
    fn to_value_type(self) -> Result<chrono::DateTime<chrono::Utc>, ParseError> {
        (&self).to_value_type()
    }
}

impl ToValueType<chrono::DateTime<chrono::Utc>> for &DatabaseValue {
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

pub trait AsModel<T> {
    fn as_model(&self) -> T;
}

pub trait AsModelResult<T, E> {
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model(&self) -> Result<T, E>;
}

pub trait AsModelResultMapped<T, E> {
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model_mapped(&self) -> Result<Vec<T>, E>;
}

pub trait AsModelResultMappedMut<T, E> {
    /// # Errors
    ///
    /// * If the model fails to be created
    fn as_model_mapped_mut(&mut self) -> Result<Vec<T>, E>;
}

#[async_trait]
pub trait AsModelResultMappedQuery<T, E> {
    /// # Errors
    ///
    /// * If the model fails to be created
    async fn as_model_mapped_query(&self, db: Arc<Box<dyn Database>>) -> Result<Vec<T>, E>;
}

pub trait AsModelResultMut<T, E> {
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

#[async_trait]
pub trait AsModelQuery<T> {
    async fn as_model_query(&self, db: Arc<Box<dyn Database>>) -> Result<T, DatabaseFetchError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_value_type_u64() {
        let value = &DatabaseValue::UInt64(123);

        assert_eq!(ToValueType::<u64>::to_value_type(value).unwrap(), 123_u64);
    }

    #[test]
    fn test_to_value_option_string_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UInt64(123))],
        };
        assert_eq!(row.to_value::<Option<String>>("bob").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_u64_where_property_doesnt_exist() {
        let row = Row {
            columns: vec![("test".to_string(), DatabaseValue::UInt64(123))],
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

    #[test]
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

    #[test]
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
}
