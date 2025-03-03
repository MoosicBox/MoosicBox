use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use moosicbox_database::{Database, DatabaseValue, Row};
use thiserror::Error;

use crate::{MissingValue, ParseError, ToValueType};

#[derive(Debug, Error)]
pub enum DatabaseFetchError {
    #[error("Invalid Request")]
    InvalidRequest,
    #[error(transparent)]
    Database(#[from] moosicbox_database::DatabaseError),
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
            | DatabaseValue::NumberOpt(None)
            | DatabaseValue::UNumberOpt(None)
            | DatabaseValue::RealOpt(None) => Ok(None),
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
            DatabaseValue::Number(num) => Ok(*num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for &DatabaseValue {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
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
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Number(num) => Ok(*num as i8),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for &DatabaseValue {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Number(num) => Ok(*num as i16),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for &DatabaseValue {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Number(num) => Ok(*num as i32),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for &DatabaseValue {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            DatabaseValue::Number(num) => Ok(*num),
            #[allow(clippy::cast_possible_wrap)]
            DatabaseValue::UNumber(num) => Ok(*num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for &DatabaseValue {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Number(num) => Ok(*num as isize),
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_possible_wrap
            )]
            DatabaseValue::UNumber(num) => Ok(*num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for &DatabaseValue {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Number(num) => Ok(*num as u8),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for &DatabaseValue {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Number(num) => Ok(*num as u16),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for &DatabaseValue {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Number(num) => Ok(*num as u32),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::UNumber(num) => Ok(*num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for &DatabaseValue {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Number(num) => Ok(*num as u64),
            DatabaseValue::UNumber(num) => Ok(*num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for &DatabaseValue {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            DatabaseValue::Number(num) => Ok(*num as usize),
            #[allow(clippy::cast_possible_truncation)]
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
        moosicbox_database::Row::get(self, index)
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
            | Self::NumberOpt(None)
            | Self::UNumberOpt(None)
            | Self::RealOpt(None) => Ok(None),
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
            Self::Number(num) => Ok(num == 1),
            _ => Err(ParseError::ConvertType("bool".into())),
        }
    }
}

impl ToValueType<f32> for DatabaseValue {
    fn to_value_type(self) -> Result<f32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Real(num) => Ok(num as f32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<f64> for DatabaseValue {
    fn to_value_type(self) -> Result<f64, ParseError> {
        match self {
            Self::Real(num) => Ok(num),
            _ => Err(ParseError::ConvertType("f64".into())),
        }
    }
}

impl ToValueType<i8> for DatabaseValue {
    fn to_value_type(self) -> Result<i8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(num) => Ok(num as i8),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as i8),
            _ => Err(ParseError::ConvertType("i8".into())),
        }
    }
}

impl ToValueType<i16> for DatabaseValue {
    fn to_value_type(self) -> Result<i16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(num) => Ok(num as i16),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as i16),
            _ => Err(ParseError::ConvertType("i16".into())),
        }
    }
}

impl ToValueType<i32> for DatabaseValue {
    fn to_value_type(self) -> Result<i32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(num) => Ok(num as i32),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as i32),
            _ => Err(ParseError::ConvertType("i32".into())),
        }
    }
}

impl ToValueType<i64> for DatabaseValue {
    fn to_value_type(self) -> Result<i64, ParseError> {
        match self {
            Self::Number(num) => Ok(num),
            #[allow(clippy::cast_possible_wrap)]
            Self::UNumber(num) => Ok(num as i64),
            _ => Err(ParseError::ConvertType("i64".into())),
        }
    }
}

impl ToValueType<isize> for DatabaseValue {
    fn to_value_type(self) -> Result<isize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(num) => Ok(num as isize),
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            Self::UNumber(num) => Ok(num as isize),
            _ => Err(ParseError::ConvertType("isize".into())),
        }
    }
}

impl ToValueType<u8> for DatabaseValue {
    fn to_value_type(self) -> Result<u8, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Number(num) => Ok(num as u8),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as u8),
            _ => Err(ParseError::ConvertType("u8".into())),
        }
    }
}

impl ToValueType<u16> for DatabaseValue {
    fn to_value_type(self) -> Result<u16, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Number(num) => Ok(num as u16),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as u16),
            _ => Err(ParseError::ConvertType("u16".into())),
        }
    }
}

impl ToValueType<u32> for DatabaseValue {
    fn to_value_type(self) -> Result<u32, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Number(num) => Ok(num as u32),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as u32),
            _ => Err(ParseError::ConvertType("u32".into())),
        }
    }
}

impl ToValueType<u64> for DatabaseValue {
    fn to_value_type(self) -> Result<u64, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Number(num) => Ok(num as u64),
            Self::UNumber(num) => Ok(num),
            _ => Err(ParseError::ConvertType("u64".into())),
        }
    }
}

impl ToValueType<usize> for DatabaseValue {
    fn to_value_type(self) -> Result<usize, ParseError> {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Number(num) => Ok(num as usize),
            #[allow(clippy::cast_possible_truncation)]
            Self::UNumber(num) => Ok(num as usize),
            _ => Err(ParseError::ConvertType("usize".into())),
        }
    }
}

impl ToValueType<NaiveDateTime> for DatabaseValue {
    fn to_value_type(self) -> Result<NaiveDateTime, ParseError> {
        match self {
            Self::DateTime(value) => Ok(value),
            _ => Err(ParseError::ConvertType("NaiveDateTime".into())),
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
        for<'b> &'b moosicbox_database::Row: ToValueType<T>;
}

impl<T, E> AsModelResultMut<T, E> for Vec<moosicbox_database::Row>
where
    E: From<DatabaseFetchError>,
{
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        for<'b> &'b moosicbox_database::Row: ToValueType<T>,
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
