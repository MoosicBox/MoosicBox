//! Core state store implementation with in-memory caching
//!
//! This module provides the [`StateStore`] type, which combines an in-memory cache
//! with a pluggable persistence backend to provide fast access to frequently used
//! state while ensuring durability through persistent storage.

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{Error, persistence::StatePersistence};

/// In-memory state store that can be optionally backed by persistent storage
pub struct StateStore<P: StatePersistence> {
    persistence: Arc<P>,
    cache: Arc<RwLock<BTreeMap<String, Value>>>,
}

impl<P: StatePersistence> StateStore<P> {
    /// Create a new state store with the given persistence backend
    #[must_use]
    pub fn new(persistence: P) -> Self {
        Self {
            persistence: Arc::new(persistence),
            cache: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Set a value in the store
    ///
    /// The value is stored in both the in-memory cache and the persistence backend.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the value cannot be serialized to JSON
    /// * [`Error::Database`] - If the persistence backend database operation fails
    /// * [`Error::InvalidDbConfiguration`] - If the persistence backend database is misconfigured
    pub async fn set<T: Serialize + Send + Sync>(
        &self,
        key: impl Into<String> + Send + Sync,
        value: &T,
    ) -> Result<(), Error> {
        let key = key.into();

        let serialized = serde_json::to_value(value)?;
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key.clone(), serialized.clone());
        }
        self.persistence.set(key, &serialized).await
    }

    /// Get a value from the store
    ///
    /// Checks the in-memory cache first, then falls back to the persistence backend
    /// if not found in cache. Returns `None` if the key does not exist.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the persistence backend database operation fails
    /// * [`Error::InvalidDbConfiguration`] - If the persistence backend database is misconfigured
    pub async fn get<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error> {
        let key = key.as_ref();

        if let Ok(cache) = self.cache.read()
            && let Some(data) = cache.get(key)
        {
            let data = serde_json::from_value(data.clone())?;
            return Ok(Some(data));
        }

        let Some(data) = self.persistence.get::<T>(key).await? else {
            return Ok(None);
        };

        let value = serde_json::to_value(data)?;

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key.to_string(), value.clone());
        }

        Ok(Some(serde_json::from_value(value)?))
    }

    /// Remove a value from the store
    ///
    /// Removes the value from both the in-memory cache and the persistence backend.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized during removal
    /// * [`Error::Database`] - If the persistence backend database operation fails
    /// * [`Error::InvalidDbConfiguration`] - If the persistence backend database is misconfigured
    pub async fn remove(&self, key: impl AsRef<str> + Send + Sync) -> Result<(), Error> {
        let key = key.as_ref();

        if let Ok(mut cache) = self.cache.write() {
            cache.remove(key);
        }
        self.persistence.remove(key).await
    }

    /// Remove a value from the store and return it
    ///
    /// Removes the value from both the in-memory cache and the persistence backend,
    /// returning the value if it exists. Returns `None` if the key does not exist.
    ///
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the persistence backend database operation fails
    /// * [`Error::InvalidDbConfiguration`] - If the persistence backend database is misconfigured
    pub async fn take<T: DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error> {
        let key = key.as_ref();

        if let Ok(mut cache) = self.cache.write() {
            cache.remove(key);
        }
        self.persistence.take(key).await
    }

    /// Clear all values from the store
    ///
    /// Removes all values from both the in-memory cache and the persistence backend.
    ///
    /// # Errors
    ///
    /// * [`Error::Database`] - If the persistence backend database operation fails
    pub async fn clear(&self) -> Result<(), Error> {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        self.persistence.clear().await
    }
}
