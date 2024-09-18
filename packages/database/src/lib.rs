#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

pub mod config;
#[cfg(feature = "postgres-raw")]
pub mod postgres;
pub mod profiles;
#[cfg(feature = "sqlite-rusqlite")]
pub mod rusqlite;
#[cfg(feature = "sqlx")]
pub mod sqlx;

pub mod query;

use std::{num::TryFromIntError, sync::Arc};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use query::{
    BooleanExpression, DeleteStatement, Expression, ExpressionType, InsertStatement, Join,
    SelectQuery, Sort, SortDirection, UpdateStatement, UpsertMultiStatement, UpsertStatement,
};
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
    Now,
    DateTime(NaiveDateTime),
}

impl DatabaseValue {
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) | Self::StringOpt(Some(value)) => Some(value),
            _ => None,
        }
    }
}

impl<T: Into<Self>> From<Option<T>> for DatabaseValue {
    fn from(val: Option<T>) -> Self {
        val.map_or(Self::Null, std::convert::Into::into)
    }
}

impl From<bool> for DatabaseValue {
    fn from(val: bool) -> Self {
        Self::Bool(val)
    }
}

impl From<&str> for DatabaseValue {
    fn from(val: &str) -> Self {
        Self::String(val.to_string())
    }
}

impl From<&String> for DatabaseValue {
    fn from(val: &String) -> Self {
        Self::String(val.to_string())
    }
}

impl From<String> for DatabaseValue {
    fn from(val: String) -> Self {
        Self::String(val)
    }
}

impl From<f32> for DatabaseValue {
    fn from(val: f32) -> Self {
        Self::Real(f64::from(val))
    }
}

impl From<f64> for DatabaseValue {
    fn from(val: f64) -> Self {
        Self::Real(val)
    }
}

impl From<i8> for DatabaseValue {
    fn from(val: i8) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i16> for DatabaseValue {
    fn from(val: i16) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i32> for DatabaseValue {
    fn from(val: i32) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i64> for DatabaseValue {
    fn from(val: i64) -> Self {
        Self::Number(val)
    }
}

impl From<isize> for DatabaseValue {
    fn from(val: isize) -> Self {
        Self::Number(val as i64)
    }
}

impl From<u8> for DatabaseValue {
    fn from(val: u8) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u16> for DatabaseValue {
    fn from(val: u16) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u32> for DatabaseValue {
    fn from(val: u32) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u64> for DatabaseValue {
    fn from(val: u64) -> Self {
        Self::UNumber(val)
    }
}

impl From<usize> for DatabaseValue {
    fn from(val: usize) -> Self {
        Self::UNumber(val as u64)
    }
}

pub trait AsId {
    fn as_id(&self) -> DatabaseValue;
}

#[derive(Debug, Error)]
pub enum TryFromError {
    #[error("Could not convert to type '{0}'")]
    CouldNotConvert(String),
    #[error(transparent)]
    TryFromInt(#[from] TryFromIntError),
}

impl TryFrom<DatabaseValue> for u64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) | DatabaseValue::NumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UNumber(value) | DatabaseValue::UNumberOpt(Some(value)) => Ok(value),
            _ => Err(TryFromError::CouldNotConvert("u64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) | DatabaseValue::NumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UNumber(value) | DatabaseValue::UNumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i32".into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "sqlite-rusqlite")]
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
    #[cfg(feature = "mysql-sqlx")]
    #[error(transparent)]
    MysqlSqlx(sqlx::mysql::SqlxDatabaseError),
    #[cfg(feature = "sqlite-sqlx")]
    #[error(transparent)]
    SqliteSqlx(sqlx::sqlite::SqlxDatabaseError),
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(postgres::postgres::PostgresDatabaseError),
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(sqlx::postgres::SqlxDatabaseError),
    #[error("No row")]
    NoRow,
}

impl DatabaseError {
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        match self {
            #[cfg(feature = "postgres-sqlx")]
            Self::PostgresSqlx(sqlx::postgres::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(
                ref _io_err,
            ))) => true,
            #[cfg(feature = "mysql-sqlx")]
            Self::MysqlSqlx(sqlx::mysql::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(
                ref _io_err,
            ))) => true,
            #[cfg(feature = "sqlite-sqlx")]
            Self::SqliteSqlx(sqlx::sqlite::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(
                ref _io_err,
            ))) => true,
            #[cfg(feature = "postgres-raw")]
            Self::Postgres(postgres::postgres::PostgresDatabaseError::Postgres(ref pg_err)) => {
                pg_err.to_string().as_str() == "connection closed"
            }
            #[cfg(feature = "sqlite-rusqlite")]
            Self::Rusqlite(rusqlite::RusqliteDatabaseError::Rusqlite(
                ::rusqlite::Error::SqliteFailure(_, _),
            )) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Row {
    pub columns: Vec<(String, DatabaseValue)>,
}

impl Row {
    #[must_use]
    pub fn get(&self, column_name: &str) -> Option<DatabaseValue> {
        self.columns
            .iter()
            .find(|c| c.0 == column_name)
            .map(|c| c.1.clone())
    }

    #[must_use]
    pub fn id(&self) -> Option<DatabaseValue> {
        self.get("id")
    }
}

#[async_trait]
pub trait Database: Send + Sync + std::fmt::Debug {
    fn select(&self, table_name: &'static str) -> SelectQuery<'static> {
        query::select(table_name)
    }
    fn update<'a>(&self, table_name: &'a str) -> UpdateStatement<'a> {
        query::update(table_name)
    }
    fn insert<'a>(&self, table_name: &'a str) -> InsertStatement<'a> {
        query::insert(table_name)
    }
    fn upsert<'a>(&self, table_name: &'a str) -> UpsertStatement<'a> {
        query::upsert(table_name)
    }
    fn upsert_first<'a>(&self, table_name: &'a str) -> UpsertStatement<'a> {
        query::upsert(table_name)
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
    async fn exec_insert(&self, statement: &InsertStatement<'_>) -> Result<Row, DatabaseError>;
    async fn exec_upsert(&self, statement: &UpsertStatement<'_>)
        -> Result<Vec<Row>, DatabaseError>;
    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError>;
    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError>;
    async fn exec_delete(&self, statement: &DeleteStatement<'_>)
        -> Result<Vec<Row>, DatabaseError>;
    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError>;

    /// # Errors
    ///
    /// Will return `Err` if the close failed to trigger.
    fn trigger_close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        self.trigger_close()
    }
}

#[async_trait]
pub trait TryFromDb<T>
where
    Self: Sized,
{
    type Error;

    async fn try_from_db(value: T, db: Arc<Box<dyn Database>>) -> Result<Self, Self::Error>;
}

#[async_trait]
impl<T, U: Send + 'static> TryFromDb<Vec<U>> for Vec<T>
where
    T: TryFromDb<U> + Send,
{
    type Error = T::Error;

    async fn try_from_db(value: Vec<U>, db: Arc<Box<dyn Database>>) -> Result<Self, T::Error> {
        let mut converted = Self::with_capacity(value.len());

        for x in value {
            converted.push(T::try_from_db(x, db.clone()).await?);
        }

        Ok(converted)
    }
}

#[async_trait]
impl<T, U: Send + 'static> TryFromDb<Option<U>> for Option<T>
where
    T: TryFromDb<U>,
{
    type Error = T::Error;

    async fn try_from_db(value: Option<U>, db: Arc<Box<dyn Database>>) -> Result<Self, T::Error> {
        Ok(match value {
            Some(x) => Some(T::try_from_db(x, db).await?),
            None => None,
        })
    }
}

#[async_trait]
pub trait TryIntoDb<T>
where
    Self: Sized,
{
    type Error;

    async fn try_into_db(self, db: Arc<Box<dyn Database>>) -> Result<T, Self::Error>;
}

#[async_trait]
impl<T: Send, U> TryIntoDb<U> for T
where
    U: TryFromDb<T>,
{
    type Error = U::Error;

    async fn try_into_db(self, db: Arc<Box<dyn Database>>) -> Result<U, U::Error> {
        U::try_from_db(self, db).await
    }
}
