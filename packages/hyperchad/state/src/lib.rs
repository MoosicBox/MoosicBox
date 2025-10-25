#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod persistence;
mod store;

pub use persistence::*;
pub use store::StateStore;

/// Errors that can occur when working with state storage
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "persistence-sqlite")]
    #[error(transparent)]
    Database(#[from] switchy::database::DatabaseError),
    #[cfg(feature = "persistence-sqlite")]
    #[error(transparent)]
    InitDb(#[from] switchy::database_connection::InitDbError),
    #[cfg(feature = "persistence-sqlite")]
    #[error("Invalid database configuration")]
    InvalidDbConfiguration,
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
