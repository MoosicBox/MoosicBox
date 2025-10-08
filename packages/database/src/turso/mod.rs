#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TursoDatabaseError {
    #[error("Turso error: {0}")]
    Turso(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Transaction error: {0}")]
    Transaction(String),
}
