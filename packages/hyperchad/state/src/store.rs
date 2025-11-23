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

#[cfg(feature = "persistence-sqlite")]
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    use crate::sqlite::SqlitePersistence;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: u32,
        name: String,
    }

    #[test_log::test(switchy_async::test)]
    async fn test_cache_hit_after_get() -> Result<(), Error> {
        // Test that values retrieved from persistence are cached for subsequent gets
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };

        // First set - should write to both cache and persistence
        store.set("key1", &data).await?;

        // First get - should hit persistence and populate cache
        let retrieved1: Option<TestData> = store.get("key1").await?;
        assert_eq!(retrieved1, Some(data.clone()));

        // Second get - should hit cache (we can't directly verify this, but it exercises the cache path)
        let retrieved2: Option<TestData> = store.get("key1").await?;
        assert_eq!(retrieved2, Some(data));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_cache_invalidation_on_remove() -> Result<(), Error> {
        // Test that cache is properly invalidated when removing items
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data = TestData {
            id: 2,
            name: "test".to_string(),
        };

        // Set and get to populate cache
        store.set("key2", &data).await?;
        let _: Option<TestData> = store.get("key2").await?;

        // Remove should clear from both cache and persistence
        store.remove("key2").await?;

        // Get should now return None
        let retrieved: Option<TestData> = store.get("key2").await?;
        assert_eq!(retrieved, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_cache_invalidation_on_clear() -> Result<(), Error> {
        // Test that cache is properly cleared when clearing the entire store
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data1 = TestData {
            id: 1,
            name: "first".to_string(),
        };
        let data2 = TestData {
            id: 2,
            name: "second".to_string(),
        };

        // Set multiple items and populate cache
        store.set("key1", &data1).await?;
        store.set("key2", &data2).await?;
        let _: Option<TestData> = store.get("key1").await?;
        let _: Option<TestData> = store.get("key2").await?;

        // Clear should remove all items from cache and persistence
        store.clear().await?;

        // Both keys should return None
        assert_eq!(store.get::<TestData>("key1").await?, None);
        assert_eq!(store.get::<TestData>("key2").await?, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_take_removes_from_cache_and_returns_value() -> Result<(), Error> {
        // Test that take removes from both cache and persistence while returning the value
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data = TestData {
            id: 3,
            name: "test".to_string(),
        };

        // Set and get to populate cache
        store.set("key3", &data).await?;
        let _: Option<TestData> = store.get("key3").await?;

        // Take should return the value and remove it
        let taken: Option<TestData> = store.take("key3").await?;
        assert_eq!(taken, Some(data));

        // Subsequent get should return None
        let retrieved: Option<TestData> = store.get("key3").await?;
        assert_eq!(retrieved, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_take_nonexistent_key_returns_none() -> Result<(), Error> {
        // Test that taking a nonexistent key returns None without errors
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let taken: Option<TestData> = store.take("nonexistent").await?;
        assert_eq!(taken, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_update_existing_key() -> Result<(), Error> {
        // Test that setting an existing key updates both cache and persistence
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data1 = TestData {
            id: 1,
            name: "original".to_string(),
        };
        let data2 = TestData {
            id: 1,
            name: "updated".to_string(),
        };

        // Set initial value
        store.set("key4", &data1).await?;
        let retrieved1: Option<TestData> = store.get("key4").await?;
        assert_eq!(retrieved1, Some(data1));

        // Update the value
        store.set("key4", &data2).await?;
        let retrieved2: Option<TestData> = store.get("key4").await?;
        assert_eq!(retrieved2, Some(data2));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_empty_string_key() -> Result<(), Error> {
        // Test that empty string keys are handled correctly
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data = TestData {
            id: 5,
            name: "empty_key_test".to_string(),
        };

        // Empty string should be a valid key
        store.set("", &data).await?;
        let retrieved: Option<TestData> = store.get("").await?;
        assert_eq!(retrieved, Some(data));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_special_characters_in_key() -> Result<(), Error> {
        // Test that keys with special characters are handled correctly
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data = TestData {
            id: 6,
            name: "special".to_string(),
        };

        let special_key = "key/with:special@chars#$%";
        store.set(special_key, &data).await?;
        let retrieved: Option<TestData> = store.get(special_key).await?;
        assert_eq!(retrieved, Some(data));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_complex_nested_data() -> Result<(), Error> {
        // Test serialization and deserialization of complex nested structures
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct ComplexData {
            items: Vec<TestData>,
            metadata: BTreeMap<String, String>,
        }

        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let mut metadata = BTreeMap::new();
        metadata.insert("version".to_string(), "1.0".to_string());
        metadata.insert("author".to_string(), "test".to_string());

        let complex = ComplexData {
            items: vec![
                TestData {
                    id: 1,
                    name: "first".to_string(),
                },
                TestData {
                    id: 2,
                    name: "second".to_string(),
                },
            ],
            metadata,
        };

        store.set("complex", &complex).await?;
        let retrieved: Option<ComplexData> = store.get("complex").await?;
        assert_eq!(retrieved, Some(complex));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_multiple_independent_keys() -> Result<(), Error> {
        // Test that multiple keys can coexist independently
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let data1 = TestData {
            id: 1,
            name: "first".to_string(),
        };
        let data2 = TestData {
            id: 2,
            name: "second".to_string(),
        };
        let data3 = TestData {
            id: 3,
            name: "third".to_string(),
        };

        // Set multiple keys
        store.set("key_a", &data1).await?;
        store.set("key_b", &data2).await?;
        store.set("key_c", &data3).await?;

        // Verify all keys are independently retrievable
        assert_eq!(store.get::<TestData>("key_a").await?, Some(data1.clone()));
        assert_eq!(store.get::<TestData>("key_b").await?, Some(data2));
        assert_eq!(store.get::<TestData>("key_c").await?, Some(data3.clone()));

        // Remove one key and verify others are unaffected
        store.remove("key_b").await?;
        assert_eq!(store.get::<TestData>("key_a").await?, Some(data1));
        assert_eq!(store.get::<TestData>("key_b").await?, None);
        assert_eq!(store.get::<TestData>("key_c").await?, Some(data3));

        Ok(())
    }
}
