//! State management with pluggable persistence backends
//!
//! This crate provides an in-memory state store with optional persistent storage
//! backends. The state store supports key-value storage with type-safe serialization
//! and deserialization of values.
//!
//! # Features
//!
//! * `persistence-sqlite` - SQLite-backed persistence using the `switchy` database library
//! * `persistence-ios` - iOS-specific persistence implementation
//!
//! # Examples
//!
//! Basic usage with `SQLite` persistence:
//!
//! ```rust,no_run
//! # #[cfg(feature = "persistence-sqlite")]
//! # {
//! use hyperchad_state::{StateStore, sqlite::SqlitePersistence};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct AppConfig {
//!     theme: String,
//!     notifications: bool,
//! }
//!
//! # async fn example() -> Result<(), hyperchad_state::Error> {
//! // Create a persistence backend
//! let persistence = SqlitePersistence::new_in_memory().await?;
//!
//! // Create the state store
//! let store = StateStore::new(persistence);
//!
//! // Store a value
//! let config = AppConfig {
//!     theme: "dark".to_string(),
//!     notifications: true,
//! };
//! store.set("config", &config).await?;
//!
//! // Retrieve a value
//! let loaded: Option<AppConfig> = store.get("config").await?;
//! # Ok(())
//! # }
//! # }
//! ```

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
