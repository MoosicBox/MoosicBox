#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "rusqlite")]
pub mod rusqlite;

pub mod query;

use async_trait::async_trait;
use query::*;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseValue {
    Null,
    String(String),
    StringOpt(Option<String>),
    Bool(bool),
    BoolOpt(Option<bool>),
    Number(i64),
    NumberOpt(Option<i64>),
    UNumber(u64),
    UNumberOpt(Option<u64>),
    Real(f64),
    RealOpt(Option<f64>),
    NowAdd(String),
}

impl DatabaseValue {
    fn to_sql(&self) -> String {
        match self {
            DatabaseValue::Null => format!("NULL"),
            DatabaseValue::BoolOpt(None) => format!("NULL"),
            DatabaseValue::StringOpt(None) => format!("NULL"),
            DatabaseValue::NumberOpt(None) => format!("NULL"),
            DatabaseValue::UNumberOpt(None) => format!("NULL"),
            DatabaseValue::RealOpt(None) => format!("NULL"),
            DatabaseValue::NowAdd(add) => {
                format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
            }
            _ => format!("?"),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value)) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromError {
    #[error("Could not convert to type '{0}'")]
    CouldNotConvert(String),
}

impl TryFrom<DatabaseValue> for u64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) => Ok(value as u64),
            DatabaseValue::NumberOpt(Some(value)) => Ok(value as u64),
            DatabaseValue::UNumber(value) => Ok(value as u64),
            DatabaseValue::UNumberOpt(Some(value)) => Ok(value as u64),
            _ => Err(TryFromError::CouldNotConvert("u64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) => Ok(value as i32),
            DatabaseValue::NumberOpt(Some(value)) => Ok(value as i32),
            DatabaseValue::UNumber(value) => Ok(value as i32),
            DatabaseValue::UNumberOpt(Some(value)) => Ok(value as i32),
            _ => Err(TryFromError::CouldNotConvert("i32".into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "rusqlite")]
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
}

pub struct DbConnection {
    #[cfg(feature = "rusqlite")]
    pub inner: ::rusqlite::Connection,
}

impl std::fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DbConnection")
    }
}

#[derive(Debug)]
pub struct Row {
    pub columns: Vec<(String, DatabaseValue)>,
}

impl Row {
    pub fn get(&self, column_name: &str) -> Option<DatabaseValue> {
        self.columns
            .iter()
            .find(|c| c.0 == column_name)
            .map(|c| c.1.clone())
    }

    pub fn id(&self) -> Option<DatabaseValue> {
        self.get("id")
    }
}

#[async_trait]
pub trait Database: Send + Sync {
    async fn select(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn select_distinct(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn select_first(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Option<Row>, DatabaseError>;

    async fn delete(
        &self,
        table_name: &str,
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn upsert(
        &self,
        table_name: &str,
        values: &[(&str, DatabaseValue)],
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<Row, DatabaseError>;

    async fn upsert_multi(
        &self,
        table_name: &str,
        unique: &[&str],
        values: &[Vec<(&str, DatabaseValue)>],
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn update_and_get_row(
        &self,
        table_name: &str,
        id: DatabaseValue,
        values: &[(&str, DatabaseValue)],
    ) -> Result<Option<Row>, DatabaseError>;
}
