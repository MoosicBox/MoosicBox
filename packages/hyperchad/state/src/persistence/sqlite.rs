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

    #[switchy_async::test]
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
}
