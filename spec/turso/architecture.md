# Turso Database Backend Architecture

## System Overview

The Turso Database backend integration provides a modern, async-first SQLite-compatible database for MoosicBox. Turso is a complete Rust rewrite of SQLite (not a fork), offering native async I/O, experimental concurrent writes, and built-in features for AI workloads while maintaining SQLite compatibility for schemas and queries.

```
Current Architecture (SQLite):
Application Code → switchy_database → rusqlite → SQLite (C) → File
                                   ↓ (synchronous, blocking)
                                   ↓ (single writer lock)
                                   
Proposed Architecture (Turso):
Application Code → switchy_database → Turso (Rust) → File
                                   ↓ (async, io_uring)
                                   ↓ (concurrent writes - BETA)
                                   ↓ (vector search ready)
```

**Key Architectural Difference**: Turso is not a fork or wrapper - it's a ground-up Rust implementation that reimagines SQLite for modern async runtimes and distributed systems.

## Design Goals

### Primary Objectives
- **SQLite Compatibility**: Maintain compatibility with SQLite file format, SQL dialect, and existing MoosicBox schemas
- **Async-First Design**: Native async I/O throughout the stack, eliminating blocking operations in async contexts
- **Drop-In Replacement**: Implement the `Database` trait to work seamlessly with existing MoosicBox code
- **Modern Performance**: Leverage Rust's zero-cost abstractions and async runtime for improved throughput
- **Future-Proof**: Prepare for concurrent writes (when stable) and distributed scenarios

### Secondary Objectives
- **Concurrent Writes**: Experimental `BEGIN CONCURRENT` support for multi-writer scenarios (BETA feature)
- **Vector Search**: Built-in vector similarity search for AI/RAG workloads (future enhancement)
- **Turso Cloud Sync**: Eventual integration with Turso Cloud for distributed/edge scenarios (not initial scope)
- **Change Data Capture**: CDC support for real-time change tracking (future enhancement)
- **Encryption at Rest**: Optional database encryption (experimental Turso feature)

## Component Architecture

### Core Abstractions

The implementation follows the existing `switchy_database` abstraction pattern:

```rust
// Main database struct
pub struct TursoDatabase {
    database: turso::Database,
    // Note: Turso has different connection model than rusqlite pooling
    // Connection management handled by Turso's internal async design
}

// Transaction support
pub struct TursoTransaction {
    transaction: turso::Transaction,
    // May support BEGIN CONCURRENT in future
}

// Error handling
#[derive(Debug, Error)]
pub enum TursoDatabaseError {
    #[error("Turso error: {0}")]
    Turso(String),  // Wraps turso::Error
    
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Query error: {0}")]
    Query(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
}
```

### Implementation Hierarchy

```
packages/database/
├── Cargo.toml              # Add turso feature
├── src/
│   ├── lib.rs              # Add turso module and error variant
│   ├── turso/              # New module
│   │   ├── mod.rs          # TursoDatabase implementation
│   │   └── transaction.rs  # TursoTransaction implementation
│   └── ...                 # Existing backends

packages/database_connection/
├── Cargo.toml              # Add turso feature
└── src/
    └── lib.rs              # Add init_turso_local() function
```

### Feature Configuration

```toml
# packages/database/Cargo.toml
[dependencies]
turso = { workspace = true, optional = true }

[features]
default = []

# Turso backend with SQLite-compatible placeholders
turso = [
    "_any_backend",
    "dep:turso",
    "placeholder-question-mark",  # SQLite uses ? for placeholders
]

fail-on-warnings = []
```

```toml
# packages/database_connection/Cargo.toml
[features]
default = []

turso = ["switchy_database/turso"]
database-connection-turso = ["turso"]

fail-on-warnings = []
```

```toml
# packages/switchy/Cargo.toml
[features]
database-turso = ["switchy_database/turso"]
database-connection-turso = ["switchy_database_connection/turso"]
```

## Implementation Details

### Turso Database Backend

**Purpose**: Implement `Database` trait using Turso's async API

**Design**: Leverage Turso's builder pattern and async-first architecture

```rust
use async_trait::async_trait;
use switchy_database::{Database, DatabaseError, DatabaseValue, Row};
use turso::{Builder, Connection};

pub struct TursoDatabase {
    database: turso::Database,
}

impl TursoDatabase {
    pub async fn new(path: &str) -> Result<Self, TursoDatabaseError> {
        let builder = Builder::new_local(path);
        let database = builder.build().await
            .map_err(|e| TursoDatabaseError::Turso(e.to_string()))?;
        
        Ok(Self { database })
    }
}

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
        
        // Convert params and execute query
        // Transform turso::Row to switchy Row
        todo!("Implementation in spec")
    }
    
    // ... other Database trait methods
}
```

**Key Design Decisions**:
- **No connection pooling wrapper**: Turso handles connections internally with async design
- **Builder pattern**: Use `turso::Builder::new_local()` for file-based databases
- **Error conversion**: Wrap `turso::Error` in `TursoDatabaseError` then `DatabaseError`
- **Row transformation**: Convert between `turso::Row` and `switchy_database::Row`

### Transaction Support

**Purpose**: Implement `DatabaseTransaction` trait for transactional operations

**Architecture**:
- Standard SQLite transactions initially
- Document `BEGIN CONCURRENT` for future when stable
- Support savepoints for nested transactions

```rust
pub struct TursoTransaction {
    transaction: turso::Transaction,
}

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
    
    // Query methods within transaction context
    async fn query(...) -> Result<Vec<Row>, DatabaseError> {
        // Execute within transaction
    }
}
```

### Query Building and Placeholders

**Compatibility**: Turso uses SQLite-compatible `?` placeholders

- Reuse existing SQLite query building logic from switchy_database
- Question mark placeholders: `SELECT * FROM users WHERE id = ?`
- Parameter binding via `turso::params!()` or similar API

### Schema Introspection

**Purpose**: Implement schema metadata queries

**Design**: Reuse SQLite PRAGMA queries (Turso is SQLite-compatible)

```rust
impl Database for TursoDatabase {
    async fn table_exists(&self, table: &str) -> Result<bool, DatabaseError> {
        // Use SQLite's sqlite_master table
        let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
        // ...
    }
    
    async fn get_table_columns(&self, table: &str) -> Result<Vec<String>, DatabaseError> {
        // Use PRAGMA table_info(table_name)
        let query = format!("PRAGMA table_info({})", table);
        // ...
    }
}
```

## Testing Framework

### Test Strategy

**Purpose**: Ensure correctness, compatibility, and performance

**Architecture**:
- **Unit tests**: Individual method testing within `packages/database/src/turso/`
- **Integration tests**: Full Database trait testing in `tests/`
- **Compatibility tests**: Compare behavior with rusqlite backend
- **Performance benchmarks**: Measure async I/O improvements

### Test Categories

**Core Functionality**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_table() {
        let db = TursoDatabase::new(":memory:").await.unwrap();
        db.exec("CREATE TABLE users (id INT, name TEXT)").await.unwrap();
        assert!(db.table_exists("users").await.unwrap());
    }
    
    #[tokio::test]
    async fn test_insert_and_query() {
        // Test data insertion and retrieval
    }
    
    #[tokio::test]
    async fn test_transactions() {
        // Test commit and rollback
    }
}
```

**Schema Introspection**:
```rust
#[tokio::test]
async fn test_schema_introspection() {
    // Test table_exists, get_table_columns, etc.
}
```

**Error Handling**:
```rust
#[tokio::test]
async fn test_error_conversion() {
    // Test TursoDatabaseError -> DatabaseError conversion
}
```

## Security Considerations

- **SQL Injection**: Use parameterized queries exclusively (enforced by switchy_database API)
- **File Permissions**: Turso respects filesystem permissions for database files
- **Encryption at Rest**: Future enhancement using Turso's experimental encryption feature
- **Memory Safety**: Guaranteed by Rust's ownership system

## Resource Management

- **Connections**: Managed internally by Turso's async runtime
- **Memory**: Rust's RAII ensures proper cleanup
- **File Handles**: Closed automatically when `TursoDatabase` is dropped
- **Async Tasks**: Bounded by tokio runtime configuration

## Integration Strategy

### Migration Path from rusqlite

**Phase 1**: Implement Turso backend alongside rusqlite
```toml
# Cargo.toml - both backends available
features = ["database-sqlite-rusqlite", "database-turso"]
```

**Phase 2**: Test with existing MoosicBox schemas
- Run integration tests against real schemas
- Compare query results between backends
- Validate transaction behavior

**Phase 3**: Production rollout
- Feature flag controlled: `use_turso_backend=true`
- Gradual migration of components
- Performance monitoring

### Future Enhancements

**Turso Cloud Integration** (not initial scope):
```rust
// Future: Remote database support
let builder = Builder::new_remote("https://[db].turso.io", "auth_token");
```

**Embedded Replicas** (not initial scope):
```rust
// Future: Local replica with cloud sync
let builder = Builder::new_local_replica("local.db")
    .sync_url("https://[db].turso.io")
    .auth_token("token");
```

**Vector Search** (future enhancement):
```sql
-- Turso has built-in vector search
SELECT * FROM embeddings 
WHERE vector_distance(embedding, ?1) < 0.5
ORDER BY vector_distance(embedding, ?1)
LIMIT 10;
```

## Configuration and Environment

**Database File Path**:
- Local file: `/path/to/database.db`
- In-memory: `:memory:`
- Temporary: Empty string or special temp path

**Turso-Specific Features** (future):
- `TURSO_CONCURRENT_WRITES=true` - Enable BEGIN CONCURRENT (BETA)
- `TURSO_ENCRYPTION_KEY=...` - Enable encryption at rest (experimental)

## Success Criteria

### Functional Requirements
- [ ] All `Database` trait methods implemented and working
- [ ] Transaction support with commit/rollback functional
- [ ] Savepoint support for nested transactions
- [ ] Schema introspection methods working
- [ ] SQLite file format compatibility verified
- [ ] Existing MoosicBox schemas work without modification

### Technical Requirements
- [ ] Zero clippy warnings with `fail-on-warnings`
- [ ] All unit tests pass
- [ ] Integration tests pass with real schemas
- [ ] Async I/O throughout (no blocking operations)
- [ ] Error handling complete and ergonomic

### Quality Requirements
- [ ] Test coverage > 80% for TursoDatabase implementation
- [ ] Performance equal to or better than rusqlite for async workloads
- [ ] Documentation complete with usage examples
- [ ] Migration guide from rusqlite documented
- [ ] BETA status clearly communicated in all documentation

### Compatibility Requirements
- [ ] Existing queries work unchanged
- [ ] Existing schemas work unchanged
- [ ] Placeholder syntax compatible (question marks)
- [ ] Error messages informative and actionable
