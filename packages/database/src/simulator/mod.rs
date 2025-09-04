use async_trait::async_trait;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use crate::{
    Database, DatabaseError, Row,
    query::{
        DeleteStatement, InsertStatement, SelectQuery, UpdateStatement, UpsertMultiStatement,
        UpsertStatement,
    },
    rusqlite::RusqliteDatabase,
};

/// Global mapping of database paths to simulation database instances
/// Using `BTreeMap` for deterministic iteration order
static DATABASE_REGISTRY: std::sync::LazyLock<Mutex<BTreeMap<String, Arc<RusqliteDatabase>>>> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SimulationDatabase {
    inner: Arc<RusqliteDatabase>,
}

impl SimulationDatabase {
    /// Create a new simulation database for the given path
    /// If path is None, creates an in-memory database
    /// If path already exists in registry, returns the existing database
    /// Otherwise creates a new database and registers it
    ///
    /// # Errors
    ///
    /// * If the database connection fails to open in memory
    ///
    /// # Panics
    ///
    /// * If time goes backwards
    pub fn new_for_path(path: Option<&str>) -> Result<Self, DatabaseError> {
        // Check if we already have a database for this path
        let registry = &DATABASE_REGISTRY;
        let mut registry_guard = registry.lock().unwrap();

        if let Some(path) = path
            && let Some(existing_db) = registry_guard.get(path)
        {
            return Ok(Self {
                inner: Arc::clone(existing_db),
            });
        }

        // Create a new database for this path
        let db = Self::create_new_database()?;
        if let Some(path) = path {
            registry_guard.insert(path.to_string(), Arc::clone(&db.inner));
        }
        drop(registry_guard);

        Ok(db)
    }

    /// # Errors
    ///
    /// * If the database connection fails to open in memory
    ///
    /// # Panics
    ///
    /// * If time goes backwards
    pub fn new() -> Result<Self, DatabaseError> {
        // For backwards compatibility, create a unique database each time
        // when no path is specified
        Self::create_new_database()
    }

    fn create_new_database() -> Result<Self, DatabaseError> {
        use std::sync::atomic::AtomicU64;

        static ID: AtomicU64 = AtomicU64::new(0);

        let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let db_url = format!("file:sqlx_memdb_{id}:?mode=memory&cache=shared&uri=true");

        let mut connections = Vec::new();
        for _ in 0..5 {
            let conn = ::rusqlite::Connection::open(&db_url)
                .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
            conn.busy_timeout(std::time::Duration::from_millis(10))
                .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
            connections.push(Arc::new(tokio::sync::Mutex::new(conn)));
        }

        Ok(Self {
            inner: Arc::new(RusqliteDatabase::new(connections)),
        })
    }
}

#[async_trait]
impl Database for SimulationDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError> {
        self.inner.query(query).await
    }

    async fn query_first(&self, query: &SelectQuery<'_>) -> Result<Option<Row>, DatabaseError> {
        self.inner.query_first(query).await
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_update(statement).await
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        self.inner.exec_update_first(statement).await
    }

    async fn exec_insert(&self, statement: &InsertStatement<'_>) -> Result<Row, DatabaseError> {
        self.inner.exec_insert(statement).await
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_upsert(statement).await
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        self.inner.exec_upsert_first(statement).await
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_upsert_multi(statement).await
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_delete(statement).await
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        self.inner.exec_delete_first(statement).await
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.inner.exec_raw(statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_create_table(statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_drop_table(statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_create_index(statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_drop_index(statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_alter_table(statement).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        self.inner.begin_transaction().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, query::FilterableQuery};

    #[switchy_async::test]
    async fn test_path_based_database_isolation() {
        // Create two databases with different paths
        let db1 = SimulationDatabase::new_for_path(Some("path1.db")).unwrap();
        let db2 = SimulationDatabase::new_for_path(Some("path2.db")).unwrap();

        // Create tables in both
        db1.exec_raw("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .await
            .unwrap();
        db2.exec_raw("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .await
            .unwrap();

        // Insert different data in each
        db1.insert("test")
            .value("value", "db1_data")
            .execute(&db1)
            .await
            .unwrap();
        db2.insert("test")
            .value("value", "db2_data")
            .execute(&db2)
            .await
            .unwrap();

        // Verify isolation - each database should only see its own data
        let rows1 = db1.select("test").execute(&db1).await.unwrap();
        let rows2 = db2.select("test").execute(&db2).await.unwrap();

        assert_eq!(rows1.len(), 1);
        assert_eq!(rows2.len(), 1);
        assert_eq!(rows1[0].columns[1].1, "db1_data".into());
        assert_eq!(rows2[0].columns[1].1, "db2_data".into());
    }

    #[switchy_async::test]
    async fn test_same_path_returns_same_database() {
        // Create two database instances with the same path
        let db1 = SimulationDatabase::new_for_path(Some("same_path.db")).unwrap();
        let db2 = SimulationDatabase::new_for_path(Some("same_path.db")).unwrap();

        // Create table and insert data via first instance
        db1.exec_raw("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .await
            .unwrap();
        db1.insert("test")
            .value("value", "shared_data")
            .execute(&db1)
            .await
            .unwrap();

        // Second instance should see the same data
        let rows = db2.select("test").execute(&db2).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns[1].1, "shared_data".into());
    }

    #[switchy_async::test]
    async fn test_simulator_transaction_delegation() {
        // Create SimulationDatabase
        let db = SimulationDatabase::new().unwrap();

        // Create a test table
        db.exec_raw("CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();

        // Begin a transaction - this should delegate to RusqliteDatabase
        let transaction = db.begin_transaction().await.unwrap();

        // Insert data within the transaction using the query builder
        transaction
            .insert("test_users")
            .value("name", "TestUser")
            .execute(&*transaction)
            .await
            .unwrap();

        // Query within the transaction to verify isolation
        let rows = transaction
            .select("test_users")
            .where_eq("name", "TestUser")
            .execute(&*transaction)
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows,
            vec![Row {
                columns: vec![("id".into(), 1.into()), ("name".into(), "TestUser".into())]
            }]
        );

        // Commit the transaction
        transaction.commit().await.unwrap();

        // Verify data persists after commit
        let rows_after_commit = db
            .select("test_users")
            .where_eq("name", "TestUser")
            .execute(&db)
            .await
            .unwrap();

        assert_eq!(rows_after_commit.len(), 1);
        assert_eq!(
            rows_after_commit,
            vec![Row {
                columns: vec![("id".into(), 1.into()), ("name".into(), "TestUser".into())]
            }]
        );
    }

    #[switchy_async::test]
    async fn test_simulator_transaction_rollback() {
        // Create SimulationDatabase
        let db = SimulationDatabase::new().unwrap();

        // Create a test table
        db.exec_raw("CREATE TABLE test_rollback (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
            .await
            .unwrap();

        // Insert initial data
        db.insert("test_rollback")
            .value("value", "initial")
            .execute(&db)
            .await
            .unwrap();

        // Begin a transaction
        let transaction = db.begin_transaction().await.unwrap();

        // Insert data within the transaction
        transaction
            .insert("test_rollback")
            .value("value", "transactional")
            .execute(&*transaction)
            .await
            .unwrap();

        // Verify data is visible within transaction
        let rows_in_tx = transaction
            .select("test_rollback")
            .execute(&*transaction)
            .await
            .unwrap();

        assert_eq!(rows_in_tx.len(), 2); // initial + transactional

        // Rollback the transaction
        transaction.rollback().await.unwrap();

        // Verify transactional data was rolled back
        let rows_after_rollback = db.select("test_rollback").execute(&db).await.unwrap();

        assert_eq!(rows_after_rollback.len(), 1); // Only initial data remains
        assert_eq!(
            rows_after_rollback,
            vec![Row {
                columns: vec![("id".into(), 1.into()), ("value".into(), "initial".into())]
            }]
        );
    }
}
