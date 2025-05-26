use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::Error;

#[cfg(feature = "persistence-sqlite")]
pub mod sqlite;

/// Core trait for state persistence implementations
#[async_trait]
pub trait StatePersistence: Send + Sync {
    /// Store a value with the given key
    async fn set<T: Serialize + Send + Sync>(
        &self,
        key: impl Into<String> + Send + Sync,
        value: &T,
    ) -> Result<(), Error>;

    /// Retrieve a value by key
    async fn get<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error>;

    /// Remove a value by key
    async fn remove(&self, key: &str) -> Result<(), Error>;

    /// Clear all stored values
    async fn clear(&self) -> Result<(), Error>;
}
