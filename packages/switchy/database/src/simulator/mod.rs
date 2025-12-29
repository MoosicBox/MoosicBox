//! Database simulator for testing and development
//!
//! This module provides a database simulator that delegates all operations to an underlying
//! `SQLite` database using rusqlite. It's designed for testing scenarios where you need
//! consistent, predictable database behavior without external dependencies.
//!
//! # Delegation Architecture
//!
//! The `SimulationDatabase` is a **pure delegation wrapper** around `RusqliteDatabase`:
//!
//! ## Complete Delegation
//! ALL database operations are delegated without modification:
//! - **Query operations**: `select()`, `find()`, `update()`, `insert()`, `delete()`, `upsert()`
//! - **Schema operations**: `exec_create_table()`, `drop_table()`, `alter_table()`
//! - **Introspection operations**: `table_exists()`, `get_table_info()`, `column_exists()`, `get_table_columns()`
//! - **Transaction operations**: `begin_transaction()` → returns `RusqliteTransaction`
//!
//! ## No Simulation Logic
//! The simulator does NOT provide:
//! - Mock data or fake responses
//! - Special test behavior or stubbing
//! - Query interception or modification
//! - Different behavior from rusqlite
//!
//! ## Purpose: Shared Test Databases
//! The primary purpose is **database instance sharing** during tests:
//! - Multiple test components can reference the same database by path
//! - Prevents test isolation issues from multiple database instances
//! - Ensures consistent state across test components
//!
//! # Schema Introspection Behavior
//!
//! Since the simulator delegates to `RusqliteDatabase`, ALL introspection behavior
//! is identical to the `SQLite` backend:
//!
//! ## Data Type Mappings
//! Same as `SQLite`:
//! - `INTEGER` → `BigInt`
//! - `TEXT` → `Text`
//! - `REAL` → `Double`
//! - `BOOLEAN` → `Bool`
//!
//! ## PRAGMA Commands
//! Uses the same `SQLite` PRAGMA commands:
//! - `PRAGMA table_info()` for column metadata
//! - `PRAGMA index_list()` for index information
//! - `PRAGMA foreign_key_list()` for foreign keys
//!
//! ## Limitations
//! Inherits all `SQLite` limitations:
//! - Limited auto-increment detection
//! - PRIMARY KEY doesn't imply NOT NULL
//! - Dynamic typing with type affinity
//! - Complex default value parsing limitations
//!
//! # Database Registry
//!
//! The simulator maintains a global registry of database instances:
//!
//! ```rust,ignore
//! // Creates or reuses database for path "test.db"
//! let db1 = SimulationDatabase::new(Some("test.db")).await?;
//! let db2 = SimulationDatabase::new(Some("test.db")).await?;
//! // db1 and db2 reference the same underlying SQLite database
//! ```
//!
//! ## Registry Key Behavior
//! - **File path**: `Some("path/to/file.db")` → Uses file path as registry key
//! - **In-memory**: `None` → Uses generated unique key for each instance
//! - **Path normalization**: Paths are used as-is (no canonicalization)
//!
//! ## Thread Safety
//! - Registry protected by `std::sync::Mutex`
//! - Multiple threads can safely access the same simulated database
//! - Underlying `SQLite` operations use connection pooling (from rusqlite implementation)
//!
//! # Usage in Tests
//!
//! ## Shared Database Instance
//! ```rust,ignore
//! // In test setup
//! let db = SimulationDatabase::new(Some("test_shared.db")).await?;
//! db.create_table("users").column(...).execute().await?;
//!
//! // In test component A
//! let db_a = SimulationDatabase::new(Some("test_shared.db")).await?;
//! // Sees the "users" table created above
//!
//! // In test component B
//! let db_b = SimulationDatabase::new(Some("test_shared.db")).await?;
//! // Same database instance as db_a
//! ```
//!
//! ## Isolated Test Instances
//! ```rust,ignore
//! // Each test gets unique in-memory database
//! let db1 = SimulationDatabase::new(None).await?; // Independent
//! let db2 = SimulationDatabase::new(None).await?; // Independent
//! let db3 = SimulationDatabase::new(None).await?; // Independent
//! ```
//!
//! # Transaction Behavior
//!
//! Transactions are delegated to the underlying `RusqliteDatabase`:
//! - `begin_transaction()` returns `RusqliteTransaction` (not a simulator transaction)
//! - All transaction isolation and connection pooling behavior from rusqlite applies
//! - No special simulator transaction logic

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

/// Database simulator that delegates all operations to an underlying rusqlite database
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

    /// Creates a new simulation database with a unique in-memory database
    ///
    /// Each call creates a completely isolated database instance.
    ///
    /// # Errors
    ///
    /// * If the database connection fails to open in memory
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
            connections.push(Arc::new(switchy_async::sync::Mutex::new(conn)));
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

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        self.inner.table_exists(table_name).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        self.inner.list_tables().await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        self.inner.get_table_info(table_name).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        self.inner.get_table_columns(table_name).await
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        self.inner.column_exists(table_name, column_name).await
    }

    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        // Delegate to inner database implementation
        self.inner.query_raw(query).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        self.inner.begin_transaction().await
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Delegate to inner database implementation
        self.inner.exec_raw_params(query, params).await
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Delegate to inner database implementation
        self.inner.query_raw_params(query, params).await
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
                columns: vec![
                    ("id".into(), i64::from(1).into()),
                    ("name".into(), "TestUser".into())
                ]
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
                columns: vec![
                    ("id".into(), i64::from(1).into()),
                    ("name".into(), "TestUser".into())
                ]
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
                columns: vec![
                    ("id".into(), i64::from(1).into()),
                    ("value".into(), "initial".into())
                ]
            }]
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_simulator_introspection_delegation() {
        // Create SimulationDatabase
        let db = SimulationDatabase::new().unwrap();

        // Create a test table with various column types
        db.exec_raw(
            "CREATE TABLE test_introspection (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                age INTEGER,
                score REAL DEFAULT 0.0
            )",
        )
        .await
        .unwrap();

        // Test table_exists - should delegate to rusqlite
        assert!(db.table_exists("test_introspection").await.unwrap());
        assert!(!db.table_exists("nonexistent_table").await.unwrap());

        // Test column_exists - should delegate to rusqlite
        assert!(db.column_exists("test_introspection", "id").await.unwrap());
        assert!(
            db.column_exists("test_introspection", "name")
                .await
                .unwrap()
        );
        assert!(
            !db.column_exists("test_introspection", "nonexistent")
                .await
                .unwrap()
        );
        assert!(!db.column_exists("nonexistent_table", "id").await.unwrap());

        // Test get_table_columns - should delegate to rusqlite
        let columns = db.get_table_columns("test_introspection").await.unwrap();
        assert_eq!(columns.len(), 4);

        // Verify column details (order should match CREATE TABLE)
        assert_eq!(columns[0].name, "id");
        assert!(columns[0].is_primary_key);
        assert_eq!(columns[1].name, "name");
        assert!(!columns[1].nullable);
        assert_eq!(columns[2].name, "age");
        assert!(columns[2].nullable);
        assert_eq!(columns[3].name, "score");
        assert!(columns[3].nullable);

        // Test get_table_info - should delegate to rusqlite
        let table_info = db.get_table_info("test_introspection").await.unwrap();
        assert!(table_info.is_some());

        let info = table_info.unwrap();
        assert_eq!(info.name, "test_introspection");
        assert_eq!(info.columns.len(), 4);
        assert!(info.columns.contains_key("id"));
        assert!(info.columns.contains_key("name"));
        assert!(info.columns.contains_key("age"));
        assert!(info.columns.contains_key("score"));

        // Test with nonexistent table
        let empty_columns = db.get_table_columns("nonexistent").await.unwrap();
        assert!(empty_columns.is_empty());

        let no_table_info = db.get_table_info("nonexistent").await.unwrap();
        assert!(no_table_info.is_none());
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_simulator_transaction_introspection() {
        // Create SimulationDatabase
        let db = SimulationDatabase::new().unwrap();

        // Create a test table
        db.exec_raw("CREATE TABLE tx_test (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .unwrap();

        // Begin transaction and test introspection works in transaction context
        let transaction = db.begin_transaction().await.unwrap();

        // All introspection methods should work through transaction delegation
        assert!(transaction.table_exists("tx_test").await.unwrap());
        assert!(transaction.column_exists("tx_test", "id").await.unwrap());

        let columns = transaction.get_table_columns("tx_test").await.unwrap();
        assert_eq!(columns.len(), 2);

        let table_info = transaction.get_table_info("tx_test").await.unwrap();
        assert!(table_info.is_some());

        transaction.commit().await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_simulator_path_isolation() {
        // Create two databases with different paths
        let db1 = SimulationDatabase::new_for_path(Some("introspection_path1.db")).unwrap();
        let db2 = SimulationDatabase::new_for_path(Some("introspection_path2.db")).unwrap();

        // Create different tables in each database
        db1.exec_raw("CREATE TABLE path1_table (id INTEGER, name TEXT)")
            .await
            .unwrap();
        db2.exec_raw("CREATE TABLE path2_table (id INTEGER, value TEXT)")
            .await
            .unwrap();

        // Verify isolation - each database should only see its own tables
        assert!(db1.table_exists("path1_table").await.unwrap());
        assert!(!db1.table_exists("path2_table").await.unwrap());

        assert!(db2.table_exists("path2_table").await.unwrap());
        assert!(!db2.table_exists("path1_table").await.unwrap());

        // Verify column isolation
        assert!(db1.column_exists("path1_table", "name").await.unwrap());
        assert!(!db1.column_exists("path1_table", "value").await.unwrap());

        assert!(db2.column_exists("path2_table", "value").await.unwrap());
        assert!(!db2.column_exists("path2_table", "name").await.unwrap());

        // Verify schema isolation through get_table_info
        let info1 = db1.get_table_info("path1_table").await.unwrap();
        let info2 = db2.get_table_info("path2_table").await.unwrap();

        assert!(info1.is_some());
        assert!(info2.is_some());
        assert!(info1.unwrap().columns.contains_key("name"));
        assert!(info2.unwrap().columns.contains_key("value"));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_basic() {
        let db = SimulationDatabase::new().unwrap();

        // Initially empty
        let tables = db.list_tables().await.unwrap();
        assert!(tables.is_empty(), "New database should have no tables");

        // Create some tables
        db.exec_raw("CREATE TABLE table1 (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        db.exec_raw("CREATE TABLE table2 (id INTEGER PRIMARY KEY, value REAL)")
            .await
            .unwrap();

        let mut tables = db.list_tables().await.unwrap();
        tables.sort(); // Sort for deterministic comparison
        assert_eq!(tables, vec!["table1", "table2"]);

        // Drop one table
        db.exec_raw("DROP TABLE table1").await.unwrap();

        let tables = db.list_tables().await.unwrap();
        assert_eq!(tables, vec!["table2"]);
        assert!(!tables.contains(&"table1".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_with_transactions() {
        let db = SimulationDatabase::new().unwrap();

        // Create a table outside transaction
        db.exec_raw("CREATE TABLE base_table (id INTEGER)")
            .await
            .unwrap();

        let tables = db.list_tables().await.unwrap();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"base_table".to_string()));

        // Test with transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create a table in transaction
        tx.exec_raw("CREATE TABLE tx_table (id INTEGER)")
            .await
            .unwrap();

        let tables_in_tx = tx.list_tables().await.unwrap();
        assert_eq!(tables_in_tx.len(), 2);
        assert!(tables_in_tx.contains(&"base_table".to_string()));
        assert!(tables_in_tx.contains(&"tx_table".to_string()));

        tx.rollback().await.unwrap();

        // After rollback, should be back to 1 table
        let tables_after_rollback = db.list_tables().await.unwrap();
        assert_eq!(tables_after_rollback.len(), 1);
        assert!(tables_after_rollback.contains(&"base_table".to_string()));
        assert!(!tables_after_rollback.contains(&"tx_table".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_isolation() {
        // Create two databases with different paths
        let db1 = SimulationDatabase::new_for_path(Some("isolation1.db")).unwrap();
        let db2 = SimulationDatabase::new_for_path(Some("isolation2.db")).unwrap();

        // Create different tables in each
        db1.exec_raw("CREATE TABLE db1_table (id INTEGER)")
            .await
            .unwrap();
        db2.exec_raw("CREATE TABLE db2_table (id INTEGER)")
            .await
            .unwrap();

        // Each database should only see its own tables
        let tables1 = db1.list_tables().await.unwrap();
        let tables2 = db2.list_tables().await.unwrap();

        assert_eq!(tables1.len(), 1);
        assert_eq!(tables2.len(), 1);
        assert!(tables1.contains(&"db1_table".to_string()));
        assert!(tables2.contains(&"db2_table".to_string()));
        assert!(!tables1.contains(&"db2_table".to_string()));
        assert!(!tables2.contains(&"db1_table".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_after_commit() {
        let db = SimulationDatabase::new().unwrap();

        // Begin transaction and create table
        let tx = db.begin_transaction().await.unwrap();
        tx.exec_raw("CREATE TABLE committed_table (id INTEGER)")
            .await
            .unwrap();

        // Table should be visible in transaction
        let tables_in_tx = tx.list_tables().await.unwrap();
        assert!(tables_in_tx.contains(&"committed_table".to_string()));

        tx.commit().await.unwrap();

        // Table should still be visible after commit
        let tables_after_commit = db.list_tables().await.unwrap();
        assert_eq!(tables_after_commit.len(), 1);
        assert!(tables_after_commit.contains(&"committed_table".to_string()));
    }
}
