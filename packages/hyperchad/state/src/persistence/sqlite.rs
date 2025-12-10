//! `SQLite`-backed persistence implementation
//!
//! This module provides a [`StatePersistence`] implementation using `SQLite` as the
//! underlying storage backend. It supports both in-memory and file-based databases.

use std::path::Path;

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use switchy::database::{
    Database,
    query::FilterableQuery as _,
    schema::{Column, DataType},
};

use crate::Error;

use super::StatePersistence;

/// SQLite-backed state persistence implementation
pub struct SqlitePersistence {
    db: Box<dyn Database>,
}

impl SqlitePersistence {
    /// Create a new in-memory `SQLite` persistence store
    ///
    /// # Errors
    ///
    /// * [`Error::InitDb`] - If the database connection cannot be established
    /// * [`Error::Database`] - If the state table cannot be created
    pub async fn new_in_memory() -> Result<Self, Error> {
        let db = switchy::database_connection::init(None, None).await?;

        Self::init_tables(&*db).await?;

        Ok(Self { db })
    }

    async fn init_tables(db: &dyn Database) -> Result<(), Error> {
        db.create_table("state")
            .if_not_exists(true)
            .column(Column {
                name: "key".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "value".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .execute(db)
            .await?;
        Ok(())
    }

    /// Create a new file-based `SQLite` persistence store
    ///
    /// # Errors
    ///
    /// * [`Error::InitDb`] - If the database connection cannot be established
    /// * [`Error::Database`] - If the state table cannot be created
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, Error> {
        let db = switchy::database_connection::init(Some(db_path.as_ref()), None).await?;

        Self::init_tables(&*db).await?;

        Ok(Self { db })
    }
}

#[async_trait]
impl StatePersistence for SqlitePersistence {
    /// # Errors
    ///
    /// * [`Error::Serde`] - If the value cannot be serialized to JSON
    /// * [`Error::Database`] - If the database upsert operation fails
    async fn set<T: Serialize + Send + Sync>(
        &self,
        key: impl Into<String> + Send + Sync,
        value: &T,
    ) -> Result<(), Error> {
        let key = key.into();
        let key = key.as_str();

        self.db
            .upsert("state")
            .values(vec![
                ("key", key),
                ("value", serde_json::to_string(value)?.as_str()),
            ])
            .where_eq("key", key)
            .unique(&["key"])
            .execute(&*self.db)
            .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the database select operation fails
    /// * [`Error::InvalidDbConfiguration`] - If the returned row does not contain a value column or the value is not a string
    async fn get<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error> {
        let key = key.as_ref();

        let result = self
            .db
            .select("state")
            .columns(&["value"])
            .where_eq("key", key)
            .execute_first(&*self.db)
            .await?;

        let Some(row) = result else {
            return Ok(None);
        };

        let Some(value) = row.get("value") else {
            return Err(Error::InvalidDbConfiguration);
        };

        let value_str = value.as_str().ok_or(Error::InvalidDbConfiguration)?;

        Ok(serde_json::from_str(value_str)?)
    }

    /// # Errors
    ///
    /// * [`Error::Serde`] - If the stored value cannot be deserialized from JSON
    /// * [`Error::Database`] - If the database delete operation fails
    async fn take<T: DeserializeOwned + Send + Sync>(
        &self,
        key: impl AsRef<str> + Send + Sync,
    ) -> Result<Option<T>, Error> {
        let key = key.as_ref();

        Ok(self
            .db
            .delete("state")
            .where_eq("key", key)
            .execute(&*self.db)
            .await?
            .into_iter()
            .next()
            .and_then(|x| x.get("value"))
            .and_then(|x| x.as_str().map(|x| serde_json::from_str(x)))
            .transpose()?)
    }

    /// # Errors
    ///
    /// * [`Error::Database`] - If the database delete operation fails
    async fn clear(&self) -> Result<(), Error> {
        self.db.delete("state").execute(&*self.db).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateStore;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestSettings {
        name: String,
        value: i32,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DifferentType {
        completely_different_field: Vec<bool>,
    }

    #[test_log::test(switchy_async::test)]
    async fn test_sqlite_persistence() -> Result<(), crate::Error> {
        let persistence = SqlitePersistence::new_in_memory().await?;
        let store = StateStore::new(persistence);

        let settings = TestSettings {
            name: "test".to_string(),
            value: 42,
        };

        // Test set and get
        store.set("settings", &settings).await?;
        let retrieved: TestSettings = store.get("settings").await?.unwrap();
        assert_eq!(settings, retrieved);

        // Test remove
        store.remove("settings").await?;
        assert!(matches!(
            store.get::<TestSettings>("settings").await,
            Ok(None)
        ));

        // Test clear
        store.set("settings1", &settings).await?;
        store.set("settings2", &settings).await?;
        store.clear().await?;
        assert!(matches!(
            store.get::<TestSettings>("settings1").await,
            Ok(None)
        ));
        assert!(matches!(
            store.get::<TestSettings>("settings2").await,
            Ok(None)
        ));

        Ok::<(), crate::Error>(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_trait_take_returns_correct_value() -> Result<(), crate::Error> {
        // Test that StatePersistence::take returns the deleted value directly
        // (without going through StateStore cache)
        let persistence = SqlitePersistence::new_in_memory().await?;

        let settings = TestSettings {
            name: "direct_take_test".to_string(),
            value: 123,
        };

        // Set value directly through persistence
        persistence.set("key", &settings).await?;

        // Take should return the value
        let taken: Option<TestSettings> = persistence.take("key").await?;
        assert_eq!(taken, Some(settings));

        // Value should no longer exist
        let after_take: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(after_take, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_trait_take_nonexistent_returns_none() -> Result<(), crate::Error> {
        // Test that StatePersistence::take returns None for nonexistent keys
        let persistence = SqlitePersistence::new_in_memory().await?;

        let taken: Option<TestSettings> = persistence.take("nonexistent_key").await?;
        assert_eq!(taken, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_trait_remove_default_impl() -> Result<(), crate::Error> {
        // Test the default remove implementation which calls take internally
        let persistence = SqlitePersistence::new_in_memory().await?;

        let settings = TestSettings {
            name: "remove_test".to_string(),
            value: 456,
        };

        persistence.set("key", &settings).await?;

        // Verify value exists
        let before: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(before, Some(settings));

        // Use remove (default implementation)
        persistence.remove("key").await?;

        // Value should be gone
        let after: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(after, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_upsert_updates_existing_key() -> Result<(), crate::Error> {
        // Test that set performs an upsert (insert or update)
        let persistence = SqlitePersistence::new_in_memory().await?;

        let original = TestSettings {
            name: "original".to_string(),
            value: 1,
        };
        let updated = TestSettings {
            name: "updated".to_string(),
            value: 2,
        };

        // Insert
        persistence.set("key", &original).await?;
        let first: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(first, Some(original));

        // Update (upsert)
        persistence.set("key", &updated).await?;
        let second: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(second, Some(updated));

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_clear_removes_all_entries() -> Result<(), crate::Error> {
        // Test clear removes multiple entries at once
        let persistence = SqlitePersistence::new_in_memory().await?;

        let settings1 = TestSettings {
            name: "first".to_string(),
            value: 1,
        };
        let settings2 = TestSettings {
            name: "second".to_string(),
            value: 2,
        };
        let settings3 = TestSettings {
            name: "third".to_string(),
            value: 3,
        };

        persistence.set("key1", &settings1).await?;
        persistence.set("key2", &settings2).await?;
        persistence.set("key3", &settings3).await?;

        // Verify all exist
        assert!(persistence.get::<TestSettings>("key1").await?.is_some());
        assert!(persistence.get::<TestSettings>("key2").await?.is_some());
        assert!(persistence.get::<TestSettings>("key3").await?.is_some());

        // Clear all
        persistence.clear().await?;

        // Verify all removed
        assert!(persistence.get::<TestSettings>("key1").await?.is_none());
        assert!(persistence.get::<TestSettings>("key2").await?.is_none());
        assert!(persistence.get::<TestSettings>("key3").await?.is_none());

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_get_nonexistent_key_returns_none() -> Result<(), crate::Error> {
        // Test that getting a nonexistent key returns None (not an error)
        let persistence = SqlitePersistence::new_in_memory().await?;

        let result: Option<TestSettings> = persistence.get("nonexistent").await?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_type_mismatch_returns_serde_error() -> Result<(), crate::Error> {
        // Test that deserializing to an incompatible type returns a serde error
        let persistence = SqlitePersistence::new_in_memory().await?;

        // Store a TestSettings value
        let settings = TestSettings {
            name: "test".to_string(),
            value: 42,
        };
        persistence.set("key", &settings).await?;

        // Try to retrieve it as a completely different type
        let result = persistence.get::<DifferentType>("key").await;

        // Should return a serde deserialization error
        assert!(
            matches!(result, Err(crate::Error::Serde(_))),
            "Expected Serde error, got: {result:?}"
        );

        Ok(())
    }

    #[test_log::test(switchy_async::test)]
    async fn test_persistence_take_type_mismatch_returns_serde_error() -> Result<(), crate::Error> {
        // Test that take returns serde error when deserializing to incompatible type
        // This tests a different code path from get - take uses delete + deserialize
        let persistence = SqlitePersistence::new_in_memory().await?;

        // Store a TestSettings value
        let settings = TestSettings {
            name: "take_type_mismatch".to_string(),
            value: 99,
        };
        persistence.set("key", &settings).await?;

        // Try to take it as a completely different type
        let result = persistence.take::<DifferentType>("key").await;

        // Should return a serde deserialization error
        assert!(
            matches!(result, Err(crate::Error::Serde(_))),
            "Expected Serde error, got: {result:?}"
        );

        // Note: Since take deletes before returning, the value should be gone
        // even though deserialization failed. This is an important behavior to understand.
        let after: Option<TestSettings> = persistence.get("key").await?;
        assert_eq!(after, None, "Value should be deleted even if take fails");

        Ok(())
    }
}
