//! Persistence backend trait and implementations
//!
//! This module defines the [`StatePersistence`] trait that abstracts over different
//! storage backends for state data. Implementations can use in-memory storage,
//! databases, file systems, or any other persistent storage mechanism.
//!
//! # Available Implementations
//!
//! * [`sqlite::SqlitePersistence`] - SQLite-backed persistence (requires `persistence-sqlite` feature)

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::Error;

/// SQLite-backed state persistence implementation
#[cfg(feature = "persistence-sqlite")]
pub mod sqlite;

/// Core trait for state persistence implementations
#[async_trait]
pub trait StatePersistence: Send + Sync {
    /// Store a value with the given key
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the value cannot be serialized to JSON
    /// * [`Error::Database`] - If the database operation fails (`SQLite` backend)
    /// * [`Error::InvalidDbConfiguration`] - If the database is misconfigured (`SQLite` backend)
    async fn set<T: Serialize + Send + Sync>(
        &self,
        key: impl Into<String> + Send + Sync,
        value: &T,
    ) -> Result<(), Error>;

    /// Retrieve a value by key
    ///
    /// Returns `None` if the key does not exist in storage.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the database operation fails (`SQLite` backend)
    /// * [`Error::InvalidDbConfiguration`] - If the database is misconfigured (`SQLite` backend)
    async fn get<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error>;

    /// Remove a value from the store
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized during removal
    /// * [`Error::Database`] - If the database operation fails (`SQLite` backend)
    /// * [`Error::InvalidDbConfiguration`] - If the database is misconfigured (`SQLite` backend)
    async fn remove(&self, key: impl AsRef<str> + Send + Sync) -> Result<(), Error> {
        self.take::<serde_json::Value>(key).await?;
        Ok(())
    }

    /// Remove a value by key and return the value
    ///
    /// Returns `None` if the key does not exist in storage.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the database operation fails (`SQLite` backend)
    /// * [`Error::InvalidDbConfiguration`] - If the database is misconfigured (`SQLite` backend)
    async fn take<T: DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error>;

    /// Clear all stored values
    ///
    /// # Errors
    ///
    /// * [`Error::Database`] - If the database operation fails (`SQLite` backend)
    async fn clear(&self) -> Result<(), Error>;
}
