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

pub struct SqlitePersistence {
    db: Box<dyn Database>,
}

impl SqlitePersistence {
    /// # Errors
    ///
    /// * If the database connection cannot be established
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

    /// # Errors
    ///
    /// * If the database connection cannot be established
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, Error> {
        let db = switchy::database_connection::init(Some(db_path.as_ref()), None).await?;

        Self::init_tables(&*db).await?;

        Ok(Self { db })
    }
}

#[async_trait]
impl StatePersistence for SqlitePersistence {
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

    #[tokio::test]
    async fn test_sqlite_persistence() -> Result<(), Error> {
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

        Ok(())
    }
}
