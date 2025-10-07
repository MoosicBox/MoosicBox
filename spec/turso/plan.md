# Turso Database Backend - Execution Plan

## Executive Summary

This specification details the implementation of a Turso Database backend for MoosicBox's switchy_database abstraction layer. Turso is a ground-up Rust rewrite of SQLite (not libSQL fork) that provides native async I/O, experimental concurrent writes, and SQLite compatibility. The implementation will provide a modern, async-first database option that maintains full compatibility with existing MoosicBox schemas while preparing for advanced features like concurrent writes and distributed scenarios.

**Current Status:** 🔴 **Not Started** - Initial planning phase

**Completion Estimate:** ~0% complete - Specification phase

## Status Legend

- 🔴 **Critical** - Blocks core functionality
- 🟡 **Important** - Affects user experience or API design
- 🟢 **Minor** - Nice-to-have or polish items
- ✅ **Complete** - Fully implemented and validated
- 🟡 **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## Design Decisions (RESOLVED)

### Database Choice ✅
- **Decision Point**: Use Turso Database (Rust rewrite) instead of libSQL (SQLite fork)
- **Rationale**:
  * Turso is the future direction mentioned in GitHub issue #23
  * Native Rust implementation with async-first design
  * Experimental concurrent writes capability
  * Built-in vector search for AI workloads
  * Matches "DST architecture" reference in issue
- **Alternatives Considered**:
  * libSQL: More mature but C-based fork, doesn't align with issue intent
  * Continue with rusqlite: Synchronous, blocking, single-writer

### Connection Model ✅
- **Decision Point**: No connection pooling wrapper in initial implementation
- **Rationale**:
  * Turso manages connections internally with async design
  * Different model from rusqlite's Arc<Mutex<Vec<Conn>>>
  * Let Turso handle async connection management
- **Implementation**: Single `turso::Database` instance, connections via `.connect()`

### Feature Rollout ✅
- **Decision Point**: Implement alongside existing backends, gradual rollout
- **Rationale**:
  * Allow testing without disrupting existing functionality
  * Feature flag controlled migration
  * Easy rollback if issues found
- **Alternatives Considered**: Replace rusqlite entirely (too risky)

### Concurrent Writes ✅
- **Decision Point**: Document but don't expose initially (BETA feature)
- **Rationale**:
  * Turso's `BEGIN CONCURRENT` is experimental
  * Needs stability testing before production use
  * Document for future enablement
- **Implementation**: Standard transactions initially, flag for future

### Placeholder Syntax ✅
- **Decision Point**: Use SQLite-compatible question mark placeholders
- **Rationale**:
  * Turso is SQLite-compatible
  * Reuse existing query building logic
  * Consistent with rusqlite backend
- **Implementation**: `placeholder-question-mark` feature flag

## Phase 1: Foundation (Error Types + Feature Flags) 🔴 **NOT STARTED**

**Goal:** Set up minimal compilable foundation without pulling in Turso dependency yet

**Status:** All tasks pending

### 1.1 Workspace Dependency Declaration

- [ ] Add Turso to workspace dependencies 🔴 **CRITICAL**
  - [ ] Open `/hdd/GitHub/wt-moosicbox/turso/Cargo.toml`
  - [ ] Find `[workspace.dependencies]` section
  - [ ] Add alphabetically: `turso = { version = "0.2.1" }`
  - [ ] Verify version is latest stable from https://crates.io/crates/turso
  - [ ] **DO NOT** add to any package yet - just workspace declaration

#### 1.1 Verification Checklist
- [ ] Workspace Cargo.toml has valid TOML syntax
- [ ] Run `cargo metadata | grep turso` (should appear in workspace deps)
- [ ] No packages using it yet (this is intentional)
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo machete` (no unused dependencies - none added yet)

### 1.2 Create Error Type Structure

- [ ] Create Turso module structure 🔴 **CRITICAL**
  - [ ] Create `packages/database/src/turso/` directory
  - [ ] Create `packages/database/src/turso/mod.rs` with error types ONLY:
    ```rust
    #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
    #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum TursoDatabaseError {
        #[error("Turso error: {0}")]
        Turso(String),

        #[error("Connection error: {0}")]
        Connection(String),

        #[error("Query error: {0}")]
        Query(String),

        #[error("Transaction error: {0}")]
        Transaction(String),
    }
    ```
  - [ ] **IMPORTANT**: Use `String` wrapper, NOT `turso::Error` yet (no dependency)

#### 1.2 Verification Checklist
- [ ] Module compiles without turso dependency
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database` (should still compile)

### 1.3 Integrate Error into DatabaseError

- [ ] Update switchy_database lib.rs 🔴 **CRITICAL**
  - [ ] Add to `packages/database/src/lib.rs`:
    ```rust
    #[cfg(feature = "turso")]
    pub mod turso;
    ```
  - [ ] Add variant to `DatabaseError` enum:
    ```rust
    #[cfg(feature = "turso")]
    #[error(transparent)]
    Turso(#[from] turso::TursoDatabaseError),
    ```

#### 1.3 Verification Checklist
- [ ] Code compiles without turso feature
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database` (compiles)
- [ ] Run `cargo machete` (no unused deps)

### 1.4 Add Feature Flag (No Dependency Yet)

- [ ] Add turso feature to switchy_database 🔴 **CRITICAL**
  - [ ] Edit `packages/database/Cargo.toml`
  - [ ] Add to `[features]` section:
    ```toml
    turso = ["_any_backend", "placeholder-question-mark"]
    ```
  - [ ] **DO NOT** add `dep:turso` yet!
  - [ ] Add to `fail-on-warnings` propagation if applicable

#### 1.4 Verification Checklist
- [ ] Feature compiles but does nothing yet (expected)
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles)
- [ ] Run `cargo build -p switchy_database --no-default-features --features turso` (compiles)
- [ ] Run `cargo machete` (no unused dependencies - turso not added yet)
- [ ] Verify error module is included with feature

## Phase 2: Core Database Implementation 🔴 **NOT STARTED**

**Goal:** Implement TursoDatabase struct with actual Turso dependency

**Status:** All tasks pending

### 2.1 Add Turso Dependency to Package

- [ ] Add turso to switchy_database dependencies 🔴 **CRITICAL**
  - [ ] Edit `packages/database/Cargo.toml`
  - [ ] Add to `[dependencies]` section:
    ```toml
    turso = { workspace = true, optional = true }
    ```
  - [ ] Update `[features]` section:
    ```toml
    turso = ["_any_backend", "dep:turso", "placeholder-question-mark"]
    ```
  - [ ] NOW we actually use the dependency

#### 2.1 Verification Checklist
- [ ] Dependency declared correctly
- [ ] Run `cargo tree -p switchy_database --features turso` (turso appears)
- [ ] Run `cargo build -p switchy_database --features turso` (pulls turso crate)
- [ ] Run `cargo machete` (turso not flagged - will be used next)

### 2.2 Implement TursoDatabase Struct

- [ ] Create TursoDatabase implementation 🔴 **CRITICAL**
  - [ ] Update `packages/database/src/turso/mod.rs`
  - [ ] Add imports:
    ```rust
    use async_trait::async_trait;
    use std::sync::Arc;
    use crate::{Database, DatabaseError, DatabaseValue, Row};
    ```
  - [ ] Implement TursoDatabase struct:
    ```rust
    pub struct TursoDatabase {
        database: turso::Database,
    }

    impl TursoDatabase {
        #[must_use]
        pub async fn new(path: &str) -> Result<Self, TursoDatabaseError> {
            let builder = turso::Builder::new_local(path);
            let database = builder.build().await
                .map_err(|e| TursoDatabaseError::Turso(e.to_string()))?;

            Ok(Self { database })
        }
    }
    ```
  - [ ] Update error types to use actual `turso::Error`:
    ```rust
    #[derive(Debug, Error)]
    pub enum TursoDatabaseError {
        #[error("Turso error: {0}")]
        Turso(String),  // Keep String for now, wrap turso::Error
        // ...
    }
    ```

#### 2.2 Verification Checklist
- [ ] Struct compiles
- [ ] `new()` method has correct signature
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles)
- [ ] Run `cargo machete` (turso is used)

### 2.3 Implement Database Trait (Partial - No Transactions Yet)

**CRITICAL NOTES:**
- Implement all methods EXCEPT `begin_transaction()`
- Use `unimplemented!()` for `begin_transaction()` temporarily
- This allows phase to compile while deferring transactions to Phase 3

- [ ] Implement Database trait methods 🔴 **CRITICAL**
  - [ ] Add `#[async_trait]` attribute
  - [ ] Implement query execution methods:
    ```rust
    #[async_trait]
    impl Database for TursoDatabase {
        async fn query(
            &self,
            query: &str,
            params: Vec<DatabaseValue>,
        ) -> Result<Vec<Row>, DatabaseError> {
            let conn = self.database.connect()
                .map_err(|e| DatabaseError::Turso(
                    TursoDatabaseError::Connection(e.to_string())
                ))?;

            // Convert DatabaseValue to turso params
            // Execute query
            // Convert turso::Row to switchy Row
            todo!("Convert params and rows")
        }

        async fn exec(&self, query: &str) -> Result<(), DatabaseError> {
            todo!("Execute statement")
        }

        async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
            unimplemented!("Transaction support in Phase 3")
        }

        // ... implement other required methods
    }
    ```

- [ ] Implement parameter conversion 🔴 **CRITICAL**
  - [ ] Create helper to convert `Vec<DatabaseValue>` to Turso params
  - [ ] Handle all DatabaseValue variants (String, Int, Float, Bool, Null, Bytes)

- [ ] Implement row conversion 🔴 **CRITICAL**
  - [ ] Create helper to convert `turso::Row` to `switchy_database::Row`
  - [ ] Map column names and values correctly

- [ ] Add unit tests 🔴 **CRITICAL**
  - [ ] Create `#[cfg(test)]` module
  - [ ] Test database creation (file and in-memory)
  - [ ] Test basic query execution
  - [ ] Test parameter binding
  - [ ] Test error handling
  - [ ] **Skip transaction tests** (Phase 3)
  - [ ] Example:
    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_create_database() {
            let db = TursoDatabase::new(":memory:").await;
            assert!(db.is_ok());
        }

        #[tokio::test]
        async fn test_create_table() {
            let db = TursoDatabase::new(":memory:").await.unwrap();
            let result = db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await;
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_insert_and_query() {
            let db = TursoDatabase::new(":memory:").await.unwrap();
            db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await.unwrap();

            // Test insert
            db.exec("INSERT INTO users VALUES (1, 'Alice')").await.unwrap();

            // Test query
            let rows = db.query("SELECT * FROM users", vec![]).await.unwrap();
            assert_eq!(rows.len(), 1);
        }
    }
    ```

#### 2.3 Verification Checklist
- [ ] All non-transaction Database methods implemented
- [ ] `begin_transaction()` uses `unimplemented!()` (temporary)
- [ ] Parameter conversion works for all types
- [ ] Row conversion preserves data correctly
- [ ] Unit tests pass (excluding transaction tests)
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles successfully)
- [ ] Run `cargo test -p switchy_database --features turso` (non-transaction tests pass)
- [ ] Run `cargo machete` (all dependencies used)

## Phase 3: Transaction Support 🔴 **NOT STARTED**

**Goal:** Implement DatabaseTransaction trait and complete Database implementation

**Status:** All tasks pending

### 3.1 Create TursoTransaction Implementation

- [ ] Create transaction module 🔴 **CRITICAL**
  - [ ] Create `packages/database/src/turso/transaction.rs`
  - [ ] Add clippy configuration:
    ```rust
    #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
    #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
    ```
  - [ ] Implement TursoTransaction struct:
    ```rust
    use async_trait::async_trait;
    use crate::{DatabaseTransaction, DatabaseError, DatabaseValue, Row};
    use super::TursoDatabaseError;

    pub struct TursoTransaction {
        transaction: turso::Transaction,
    }

    impl TursoTransaction {
        #[must_use]
        pub fn new(transaction: turso::Transaction) -> Self {
            Self { transaction }
        }
    }
    ```

- [ ] Implement DatabaseTransaction trait 🔴 **CRITICAL**
  - [ ] Add `#[async_trait]` attribute
  - [ ] Implement all required methods:
    ```rust
    #[async_trait]
    impl DatabaseTransaction for TursoTransaction {
        async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
            self.transaction.commit().await
                .map_err(|e| DatabaseError::Turso(
                    TursoDatabaseError::Transaction(e.to_string())
                ))
        }

        async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
            self.transaction.rollback().await
                .map_err(|e| DatabaseError::Turso(
                    TursoDatabaseError::Transaction(e.to_string())
                ))
        }

        async fn query(
            &self,
            query: &str,
            params: Vec<DatabaseValue>,
        ) -> Result<Vec<Row>, DatabaseError> {
            // Execute query within transaction context
            todo!("Query in transaction")
        }

        // ... implement other DatabaseTransaction methods
    }
    ```

- [ ] Add transaction module to turso/mod.rs 🔴 **CRITICAL**
  - [ ] Add: `pub mod transaction;`
  - [ ] Add: `pub use transaction::TursoTransaction;`

#### 3.1 Verification Checklist
- [ ] Transaction module compiles
- [ ] All DatabaseTransaction methods implemented
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles)

### 3.2 Complete Database::begin_transaction Implementation

- [ ] Replace unimplemented! with real code 🔴 **CRITICAL**
  - [ ] Update `packages/database/src/turso/mod.rs`
  - [ ] Replace `begin_transaction()` stub:
    ```rust
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let conn = self.database.connect()
            .map_err(|e| DatabaseError::Turso(
                TursoDatabaseError::Connection(e.to_string())
            ))?;

        let tx = conn.transaction().await
            .map_err(|e| DatabaseError::Turso(
                TursoDatabaseError::Transaction(e.to_string())
            ))?;

        Ok(Box::new(TursoTransaction::new(tx)))
    }
    ```

#### 3.2 Verification Checklist
- [ ] No more `unimplemented!()` in Database impl
- [ ] `begin_transaction()` returns working transaction
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles)

### 3.3 Add Transaction Tests

- [ ] Create comprehensive transaction tests 🔴 **CRITICAL**
  - [ ] Add to test module in `mod.rs`:
    ```rust
    #[tokio::test]
    async fn test_transaction_commit() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await.unwrap();

        let tx = db.begin_transaction().await.unwrap();
        tx.exec("INSERT INTO users VALUES (1, 'Alice')").await.unwrap();
        tx.commit().await.unwrap();

        let rows = db.query("SELECT * FROM users", vec![]).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await.unwrap();

        let tx = db.begin_transaction().await.unwrap();
        tx.exec("INSERT INTO users VALUES (1, 'Alice')").await.unwrap();
        tx.rollback().await.unwrap();

        let rows = db.query("SELECT * FROM users", vec![]).await.unwrap();
        assert_eq!(rows.len(), 0); // Should be empty after rollback
    }

    #[tokio::test]
    async fn test_transaction_query() {
        // Test queries within transaction context
    }
    ```

- [ ] Test savepoints if supported 🟡 **IMPORTANT**
  - [ ] Check if Turso supports savepoints
  - [ ] Add savepoint tests if available

#### 3.3 Verification Checklist
- [ ] All transaction tests written
- [ ] Commit test passes
- [ ] Rollback test passes
- [ ] Query within transaction works
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo test -p switchy_database --features turso` (all tests pass including transactions)
- [ ] Run `cargo machete` (no unused dependencies)

## Phase 4: Schema Introspection 🟡 **NOT STARTED**

**Goal:** Implement schema metadata query methods

**Status:** All tasks pending

### 4.1 Implement Schema Methods

- [ ] Implement table_exists() 🟡 **IMPORTANT**
  - [ ] Add method to TursoDatabase:
    ```rust
    async fn table_exists(&self, table: &str) -> Result<bool, DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
        let rows = self.query(query, vec![DatabaseValue::String(table.to_string())]).await?;
        Ok(!rows.is_empty())
    }
    ```

- [ ] Implement get_table_columns() 🟡 **IMPORTANT**
  - [ ] Use SQLite PRAGMA:
    ```rust
    async fn get_table_columns(&self, table: &str) -> Result<Vec<String>, DatabaseError> {
        // Use PRAGMA table_info(table_name)
        let query = format!("PRAGMA table_info({})", table);
        let rows = self.query(&query, vec![]).await?;

        // Extract column names from rows
        let columns = rows.iter()
            .filter_map(|row| row.get("name").ok())
            .map(|v| v.to_string())
            .collect();

        Ok(columns)
    }
    ```

- [ ] Implement column_exists() 🟡 **IMPORTANT**
  - [ ] Leverage get_table_columns():
    ```rust
    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table).await?;
        Ok(columns.contains(&column.to_string()))
    }
    ```

- [ ] Implement list_tables() 🟡 **IMPORTANT**
  - [ ] Query sqlite_master:
    ```rust
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name";
        let rows = self.query(query, vec![]).await?;

        let tables = rows.iter()
            .filter_map(|row| row.get("name").ok())
            .map(|v| v.to_string())
            .collect();

        Ok(tables)
    }
    ```

#### 4.1 Verification Checklist
- [ ] All schema methods implemented
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database --features turso` (compiles)

### 4.2 Add Schema Introspection Tests

- [ ] Test all schema methods 🟡 **IMPORTANT**
  - [ ] Add tests:
    ```rust
    #[tokio::test]
    async fn test_table_exists() {
        let db = TursoDatabase::new(":memory:").await.unwrap();

        assert!(!db.table_exists("users").await.unwrap());

        db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await.unwrap();

        assert!(db.table_exists("users").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_table_columns() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INTEGER, name TEXT, email TEXT)").await.unwrap();

        let columns = db.get_table_columns("users").await.unwrap();
        assert_eq!(columns.len(), 3);
        assert!(columns.contains(&"id".to_string()));
        assert!(columns.contains(&"name".to_string()));
        assert!(columns.contains(&"email".to_string()));
    }

    #[tokio::test]
    async fn test_column_exists() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await.unwrap();

        assert!(db.column_exists("users", "id").await.unwrap());
        assert!(db.column_exists("users", "name").await.unwrap());
        assert!(!db.column_exists("users", "email").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_tables() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INTEGER)").await.unwrap();
        db.exec("CREATE TABLE posts (id INTEGER)").await.unwrap();

        let tables = db.list_tables().await.unwrap();
        assert_eq!(tables.len(), 2);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"posts".to_string()));
    }
    ```

#### 4.2 Verification Checklist
- [ ] All schema tests pass
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo test -p switchy_database --features turso` (all tests pass)
- [ ] Run `cargo machete` (no unused dependencies)

## Phase 5: Connection Initialization 🟡 **NOT STARTED**

**Goal:** Add connection initialization functions to database_connection package

**Status:** All tasks pending

### 5.1 Add Features to database_connection

- [ ] Add turso feature flag 🟡 **IMPORTANT**
  - [ ] Edit `packages/database_connection/Cargo.toml`
  - [ ] Add to `[features]`:
    ```toml
    turso = ["switchy_database/turso"]
    database-connection-turso = ["turso"]
    ```
  - [ ] Ensure feature propagates to switchy_database

#### 5.1 Verification Checklist
- [ ] Feature defined correctly
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo build -p switchy_database_connection --features turso` (compiles)
- [ ] Run `cargo tree -p switchy_database_connection --features turso` (switchy_database turso feature enabled)

### 5.2 Implement init_turso_local Function

- [ ] Add initialization function 🟡 **IMPORTANT**
  - [ ] Edit `packages/database_connection/src/lib.rs`
  - [ ] Add error variant to `InitDbError`:
    ```rust
    #[cfg(feature = "turso")]
    #[error(transparent)]
    InitTurso(#[from] InitTursoError),
    ```
  - [ ] Create error type:
    ```rust
    #[cfg(feature = "turso")]
    #[derive(Debug, Error)]
    pub enum InitTursoError {
        #[error(transparent)]
        Turso(#[from] switchy_database::turso::TursoDatabaseError),
    }
    ```
  - [ ] Implement init function:
    ```rust
    #[cfg(feature = "turso")]
    pub async fn init_turso_local(
        path: Option<&std::path::Path>,
    ) -> Result<Box<dyn Database>, InitDbError> {
        let db_path = path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ":memory:".to_string());

        let db = switchy_database::turso::TursoDatabase::new(&db_path).await?;

        Ok(Box::new(db))
    }
    ```

#### 5.2 Verification Checklist
- [ ] Function compiles
- [ ] Error handling correct
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database_connection --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database_connection --features turso` (compiles)
- [ ] Run `cargo machete` (no unused dependencies)

### 5.3 Integrate with init() Function

- [ ] Update main init() function 🟡 **IMPORTANT**
  - [ ] Add turso branch to init() in `lib.rs`:
    ```rust
    pub async fn init(
        #[cfg(feature = "sqlite")]
        path: Option<&std::path::Path>,
        creds: Option<Credentials>,
    ) -> Result<Box<dyn Database>, InitDbError> {
        #[cfg(feature = "simulator")]
        {
            // existing simulator code
        }

        #[cfg(not(feature = "simulator"))]
        {
            // existing backend selection...

            if cfg!(feature = "turso") {
                #[cfg(feature = "turso")]
                return Ok(init_turso_local(path).await?);
                #[cfg(not(feature = "turso"))]
                panic!("Invalid database features")
            } else if cfg!(feature = "postgres-raw") {
                // existing postgres code
            }
            // ... rest of backends
        }
    }
    ```

#### 5.3 Verification Checklist
- [ ] Integration works correctly
- [ ] Feature selection logic correct
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo clippy --all-targets -p switchy_database_connection --features turso -- -D warnings` (zero warnings)
- [ ] Run `cargo build -p switchy_database_connection --features turso` (compiles)
- [ ] Run `cargo test -p switchy_database_connection --features turso` (tests pass)
- [ ] Run `cargo machete` (no unused dependencies)

### 5.4 Add Workspace-Level Features

- [ ] Wire features through switchy package 🟡 **IMPORTANT**
  - [ ] Edit `packages/switchy/Cargo.toml`
  - [ ] Add features:
    ```toml
    database-turso = ["switchy_database/turso"]
    database-connection-turso = ["switchy_database_connection/turso"]
    ```

#### 5.4 Verification Checklist
- [ ] Features propagate correctly
- [ ] Run `cargo fmt` (format code)
- [ ] Run `cargo build -p switchy --features database-turso` (compiles)
- [ ] Run `cargo build -p switchy --features database-connection-turso` (compiles)
- [ ] Run `cargo machete` (workspace-wide check)

## Phase 6: Integration Testing and Documentation 🟢 **NOT STARTED**

**Goal:** Comprehensive testing and documentation

**Status:** All tasks pending

### 6.1 Integration Tests

- [ ] Create integration test suite 🟢 **MINOR**
  - [ ] Create `packages/database/tests/turso_integration.rs`
  - [ ] Test with real MoosicBox schemas (if available)
  - [ ] Test compatibility with existing code
  - [ ] Example:
    ```rust
    #[cfg(feature = "turso")]
    #[tokio::test]
    async fn test_real_world_schema() {
        // Test with actual MoosicBox table structures
    }
    ```

- [ ] Performance benchmarks 🟢 **MINOR**
  - [ ] Create `packages/database/benches/turso_bench.rs`
  - [ ] Compare query performance vs rusqlite
  - [ ] Measure async I/O improvements
  - [ ] Benchmark transaction throughput

#### 6.1 Verification Checklist
- [ ] Integration tests pass
- [ ] Benchmarks complete
- [ ] Performance equal or better than rusqlite
- [ ] Run `cargo test --features turso` (all integration tests pass)

### 6.2 Documentation

- [ ] Update crate documentation 🟢 **MINOR**
  - [ ] Add module-level docs to `turso/mod.rs`:
    ```rust
    //! Turso Database backend implementation
    //!
    //! **⚠️ BETA**: Turso Database is currently in BETA.
    //! Use caution with production data.
    //!
    //! # Features
    //!
    //! * Native async I/O with io_uring support (Linux)
    //! * SQLite-compatible file format and SQL dialect
    //! * Experimental concurrent writes (not exposed yet)
    //! * Built-in vector search capability (future)
    //!
    //! # Examples
    //!
    //! ```rust,no_run
    //! use switchy_database::turso::TursoDatabase;
    //!
    //! #[tokio::main]
    //! async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //!     let db = TursoDatabase::new("database.db").await?;
    //!     db.exec("CREATE TABLE users (id INTEGER, name TEXT)").await?;
    //!     Ok(())
    //! }
    //! ```
    ```

- [ ] Create usage examples 🟢 **MINOR**
  - [ ] Create `packages/database/examples/turso_basic.rs`
  - [ ] Create `packages/database/examples/turso_transactions.rs`
  - [ ] Create migration guide from rusqlite

- [ ] Document BETA status and limitations 🟢 **MINOR**
  - [ ] Create `packages/database/docs/turso.md`
  - [ ] List known limitations
  - [ ] Document experimental features
  - [ ] Provide migration path

#### 6.2 Verification Checklist
- [ ] All documentation complete
- [ ] Examples compile and run
- [ ] Run `cargo doc --features turso` (docs build without warnings)
- [ ] Run `cargo run --example turso_basic --features turso` (example works)

## Success Criteria

The following criteria must be met for the project to be considered successful:

- [ ] All `Database` trait methods implemented and tested
- [ ] Full transaction support with commit/rollback functional
- [ ] Schema introspection methods working (table_exists, get_table_columns, etc.)
- [ ] Connection initialization via database_connection working
- [ ] All public APIs documented with examples
- [ ] Zero clippy warnings with `fail-on-warnings` enabled
- [ ] Test coverage > 80% for business logic
- [ ] Integration tests pass with real MoosicBox schemas
- [ ] Performance benchmarks show equal or better performance vs rusqlite for async workloads
- [ ] BETA status clearly documented
- [ ] Can run alongside existing backends without conflicts
- [ ] Feature flags work correctly at all levels (database, database_connection, switchy)
- [ ] Migration guide from rusqlite available

## Technical Decisions

### Language and Framework
- **Rust** with edition 2024
- **Tokio** async runtime (Turso requires it)
- **BTreeMap/BTreeSet** for all collections (never HashMap/HashSet)
- **Workspace dependencies** using `{ workspace = true }`
- **Underscore naming** for all packages

### Architecture Patterns
- **Trait-based abstraction**: Implement existing `Database` and `DatabaseTransaction` traits
- **Error wrapping**: `turso::Error` → `TursoDatabaseError` → `DatabaseError`
- **Async-first**: All methods async, no blocking operations
- **Feature flags**: Optional dependency, can coexist with other backends

### Key Design Principles
1. **Incremental Compilation**: Each phase must compile independently
2. **No Unused Dependencies**: Add dependencies only when actually used
3. **Test-Driven**: Tests written alongside implementation
4. **SQLite Compatibility**: Reuse SQLite query patterns where possible
5. **Future-Proof**: Architecture supports upcoming Turso features (concurrent writes, sync, vector search)

## Risk Mitigation

### High-Risk Areas

1. **Turso BETA Status**
   - Risk: Bugs, API changes, breaking changes in Turso crate
   - Mitigation:
     * Pin to specific Turso version
     * Comprehensive test coverage
     * Document BETA status prominently
     * Keep rusqlite backend available as fallback

2. **Async API Differences**
   - Risk: Turso's async patterns may differ from expectations
   - Mitigation:
     * Study Turso documentation thoroughly
     * Test async behavior explicitly
     * Use tokio::test for all async tests
     * Monitor for blocking operations

3. **SQLite Compatibility**
   - Risk: Turso may not be 100% SQLite-compatible
   - Mitigation:
     * Test with real MoosicBox schemas
     * Compare results with rusqlite
     * Document any incompatibilities found
     * Integration tests against existing code

4. **Parameter and Row Conversion**
   - Risk: Type mismatches between switchy and Turso types
   - Mitigation:
     * Comprehensive type conversion tests
     * Handle all DatabaseValue variants
     * Test edge cases (NULL, empty strings, large numbers)
     * Validate row data integrity

5. **Connection Management**
   - Risk: Turso's connection model may not match assumptions
   - Mitigation:
     * Study Turso's connection documentation
     * Test concurrent connection usage
     * Monitor resource usage
     * Compare with rusqlite behavior

## Notes for Implementation

### Critical Reminders
- **⚠️ BETA Software**: Turso Database is in BETA - document this prominently
- **No HashMap/HashSet**: Always use BTreeMap/BTreeSet for deterministic ordering
- **Workspace Dependencies**: Always use `{ workspace = true }` syntax
- **Phase Independence**: Each phase must compile successfully before moving to next
- **Proof Tracking**: Add proof details under completed checkboxes with file locations

### Verification Commands
Every phase must pass:
```bash
cargo fmt                                                    # Format code
cargo clippy --all-targets -p [package] -- -D warnings      # Zero warnings
cargo build -p [package] --features turso                   # Compiles
cargo test -p [package] --features turso                    # Tests pass
cargo machete                                                # No unused deps
```

### Feature Flag Testing
Test both with and without features:
```bash
cargo build -p switchy_database --features turso            # With feature
cargo build -p switchy_database --no-default-features       # Without
cargo test -p switchy_database --features turso             # Test with feature
```
