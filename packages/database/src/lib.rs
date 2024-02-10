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

impl<T: Into<DatabaseValue>> Into<DatabaseValue> for Option<T> {
    fn into(self) -> DatabaseValue {
        self.map(|x| x.into()).unwrap_or(DatabaseValue::Null)
    }
}

impl Into<DatabaseValue> for bool {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Bool(self)
    }
}

impl Into<DatabaseValue> for &str {
    fn into(self) -> DatabaseValue {
        DatabaseValue::String(self.to_string())
    }
}

impl Into<DatabaseValue> for String {
    fn into(self) -> DatabaseValue {
        DatabaseValue::String(self)
    }
}

impl Into<DatabaseValue> for f32 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Real(self as f64)
    }
}

impl Into<DatabaseValue> for f64 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Real(self)
    }
}

impl Into<DatabaseValue> for i8 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Number(self as i64)
    }
}

impl Into<DatabaseValue> for i16 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Number(self as i64)
    }
}

impl Into<DatabaseValue> for i32 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Number(self as i64)
    }
}

impl Into<DatabaseValue> for i64 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Number(self)
    }
}

impl Into<DatabaseValue> for isize {
    fn into(self) -> DatabaseValue {
        DatabaseValue::Number(self as i64)
    }
}

impl Into<DatabaseValue> for u8 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::UNumber(self as u64)
    }
}

impl Into<DatabaseValue> for u16 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::UNumber(self as u64)
    }
}

impl Into<DatabaseValue> for u32 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::UNumber(self as u64)
    }
}

impl Into<DatabaseValue> for u64 {
    fn into(self) -> DatabaseValue {
        DatabaseValue::UNumber(self)
    }
}

impl Into<DatabaseValue> for usize {
    fn into(self) -> DatabaseValue {
        DatabaseValue::UNumber(self as u64)
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
    fn select<'a>(&self, table_name: &'a str) -> SelectQuery<'a> {
        query::select(table_name)
    }
    fn update<'a>(&self, table_name: &'a str) -> UpdateStatement<'a> {
        query::update(table_name)
    }
    fn upsert<'a>(&self, table_name: &'a str) -> UpdateStatement<'a> {
        query::update(table_name)
    }
    fn upsert_multi<'a>(&self, table_name: &'a str) -> UpsertMultiStatement<'a> {
        query::upsert_multi(table_name)
    }
    fn delete<'a>(&self, table_name: &'a str) -> DeleteStatement<'a> {
        query::delete(table_name)
    }

    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError>;
    async fn query_first(&self, query: &SelectQuery<'_>) -> Result<Option<Row>, DatabaseError>;
    async fn exec_update(&self, statement: &UpdateStatement<'_>)
        -> Result<Vec<Row>, DatabaseError>;
    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError>;
    async fn exec_upsert(&self, statement: &UpdateStatement<'_>)
        -> Result<Vec<Row>, DatabaseError>;
    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError>;
    async fn exec_delete(&self, statement: &DeleteStatement<'_>)
        -> Result<Vec<Row>, DatabaseError>;
}
