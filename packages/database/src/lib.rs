#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "rusqlite")]
pub mod rusqlite;

use async_trait::async_trait;
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

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "rusqlite")]
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
}

pub struct Row {
    pub columns: Vec<(String, DatabaseValue)>,
}

#[async_trait]
pub trait Database: Send {
    async fn update_and_get_row<'a>(
        &self,
        table_name: &str,
        id: DatabaseValue,
        values: &[(&'a str, DatabaseValue)],
    ) -> Result<Option<Row>, DatabaseError>;
}
