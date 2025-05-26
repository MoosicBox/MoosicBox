use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{Error, persistence::StatePersistence};

/// In-memory state store that can be optionally backed by persistent storage
pub struct StateStore<P: StatePersistence> {
    persistence: Arc<P>,
    cache: Arc<RwLock<HashMap<String, Value>>>,
}

impl<P: StatePersistence> StateStore<P> {
    pub fn new(persistence: P) -> Self {
        Self {
            persistence: Arc::new(persistence),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a value in the store
    ///
    /// # Errors
    ///
    /// * If the value cannot be serialized
    pub async fn set<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<(), Error> {
        let serialized = serde_json::to_value(value)?;
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key.to_string(), serialized.clone());
        }
        self.persistence.set(key, &serialized).await
    }

    /// Get a value from the store
    ///
    /// # Errors
    ///
    /// * If the value cannot be deserialized
    pub async fn get<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error> {
        let key = key.as_ref();

        if let Ok(cache) = self.cache.read() {
            if let Some(data) = cache.get(key) {
                let data = serde_json::from_value(data.clone())?;
                return Ok(Some(data));
            }
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
    /// # Errors
    ///
    /// * If the value cannot removed from the underlying `StatePersistence` implementation
    pub async fn remove(&self, key: &str) -> Result<(), Error> {
        if let Ok(mut cache) = self.cache.write() {
            cache.remove(key);
        }
        self.persistence.remove(key).await
    }

    /// Clear all values from the store
    ///
    /// # Errors
    ///
    /// * If the underlying `StatePersistence` implementation cannot be cleared
    pub async fn clear(&self) -> Result<(), Error> {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        self.persistence.clear().await
    }
}
