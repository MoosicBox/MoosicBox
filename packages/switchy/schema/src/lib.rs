#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod discovery;
pub mod migration;
pub mod runner;
pub mod version;

#[cfg(feature = "validation")]
pub mod validation;

#[cfg(feature = "test-utils")]
pub mod test_utils;

use switchy_database::DatabaseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error(transparent)]
    Database(#[from] DatabaseError),

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Migration discovery failed: {0}")]
    Discovery(String),

    #[error("Migration validation failed: {0}")]
    Validation(String),

    #[error("Migration execution failed: {0}")]
    Execution(String),
}

pub type Result<T> = std::result::Result<T, MigrationError>;
