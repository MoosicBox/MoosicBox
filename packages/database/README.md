# Switchy Database

Database abstraction layer with support for multiple database backends, schema management, and transactions.

## Overview

The Switchy Database package provides:

- **Multi-Database Support**: SQLite (rusqlite and sqlx), PostgreSQL (raw and sqlx), MySQL (sqlx), and Turso
- **Schema Management**: Create/alter tables, indexes with portable definitions
- **Schema Introspection**: Query existing database structure programmatically
- **Transaction Support**: ACID transactions with savepoint capabilities for nested transaction-like behavior
- **Query Builder**: Type-safe query construction for common operations

## Features

### Database Backends

- **SQLite (rusqlite)**: File-based database using rusqlite driver with `?` placeholders
- **SQLite (sqlx)**: File-based database using sqlx driver with `?` placeholders
- **PostgreSQL (raw)**: Production PostgreSQL using tokio-postgres and deadpool-postgres
- **PostgreSQL (sqlx)**: Production PostgreSQL using sqlx with connection pooling
- **MySQL (sqlx)**: MySQL database using sqlx driver
- **Turso**: Turso (libSQL) cloud database support
- **Simulator**: Testing database (delegates to underlying backend)

### Schema Features

- **Schema Creation**: Create tables, indexes, and alter existing schema
- **Schema Introspection**: Check table/column existence, get table metadata
- **Type Portability**: Common data type abstraction across backends
- **Foreign Keys**: Define and introspect foreign key relationships
- **Auto-increment**: Backend-specific auto-increment handling

### Transaction Features

- **ACID Transactions**: Full transaction support across all backends
- **Savepoints**: Nested transaction-like behavior with rollback points
- **Connection Pooling**: Efficient connection management (backend-dependent)

## Usage

### Basic Query Operations

```rust
use switchy_database::{Database, DatabaseError};

async fn query_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // SELECT query
    let rows = db.select("tracks")
        .columns(&["id", "title", "artist"])
        .where_eq("artist", "The Beatles")
        .execute(db)
        .await?;

    // Get first row
    let row = db.select("tracks")
        .where_eq("id", 42)
        .execute_first(db)
        .await?;

    // INSERT
    let new_row = db.insert("tracks")
        .value("title", "Come Together")
        .value("artist", "The Beatles")
        .value("duration", 259)
        .execute(db)
        .await?;

    println!("Inserted track with ID: {:?}", new_row.id());

    // UPDATE
    db.update("tracks")
        .value("artist", "The Beatles (Remastered)")
        .where_eq("id", 42)
        .execute(db)
        .await?;

    // DELETE
    db.delete("tracks")
        .where_eq("id", 42)
        .execute(db)
        .await?;

    Ok(())
}
```

### Transactions

```rust
use switchy_database::{Database, DatabaseError};

async fn transaction_example(db: &dyn Database) -> Result<(), DatabaseError> {
    // Begin transaction
    let tx = db.begin_transaction().await?;

    // Execute operations within transaction
    let user_row = tx.insert("users")
        .value("username", "music_lover")
        .value("email", "user@example.com")
        .execute(&*tx)
        .await?;

    let user_id = user_row.id().and_then(|v| v.as_i64()).unwrap();

    let playlist_row = tx.insert("playlists")
        .value("user_id", user_id)
        .value("name", "My Favorites")
        .execute(&*tx)
        .await?;

    // Commit transaction
    tx.commit().await?;

    println!("Created user {} with playlist", user_id);

    Ok(())
}

async fn transaction_with_rollback(db: &dyn Database) -> Result<(), DatabaseError> {
    let tx = db.begin_transaction().await?;

    // This will succeed
    tx.insert("artists")
        .value("name", "New Artist")
        .execute(&*tx)
        .await?;

    // This might fail (e.g., duplicate key)
    let result = tx.insert("artists")
        .value("name", "New Artist") // Same name, might violate unique constraint
        .execute(&*tx)
        .await;

    match result {
        Ok(_) => {
            tx.commit().await?;
            println!("Transaction committed successfully");
        },
        Err(e) => {
            tx.rollback().await?;
            println!("Transaction rolled back due to error: {}", e);
        }
    }

    Ok(())
}
```

### Savepoints (Nested Transactions)

Savepoints allow partial rollback within a transaction, enabling complex error recovery:

```rust
use switchy_database::{Database, DatabaseError};

async fn batch_import_with_recovery(
    db: &dyn Database,
    batches: Vec<Vec<String>>
) -> Result<(), DatabaseError> {
    let tx = db.begin_transaction().await?;

    // Process records in batches with savepoints
    for (batch_num, batch) in batches.iter().enumerate() {
        let sp = tx.savepoint(&format!("batch_{}", batch_num)).await?;

        match process_batch(&*tx, batch).await {
            Ok(_) => {
                // Batch successful, merge into transaction
                sp.release().await?;
            }
            Err(e) => {
                // Batch failed, rollback this batch only
                eprintln!("Batch {} failed: {}", batch_num, e);
                sp.rollback_to().await?;
                // Transaction continues with other batches
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

async fn process_batch(tx: &dyn Database, batch: &[String]) -> Result<(), DatabaseError> {
    for item in batch {
        tx.insert("items").value("name", item).execute(tx).await?;
    }
    Ok(())
}
```

#### Backend Support

| Database   | Savepoint Support | Notes                               |
| ---------- | ----------------- | ----------------------------------- |
| SQLite     | ✅ Full           | Can create savepoints after errors  |
| PostgreSQL | ✅ Full           | Must create before potential errors |
| MySQL      | ✅ Full (InnoDB)  | Requires InnoDB storage engine      |

#### Common Use Cases

- **Batch Processing**: Process large datasets with per-batch recovery
- **Migration Testing**: Test schema changes with rollback capability
- **Complex Business Logic**: Multi-step operations with conditional rollback
- **Error Recovery**: Continue transaction after handling specific errors

### Schema Management

```rust
use switchy_database::{Database, DatabaseError};
use switchy_database::schema::{create_table, Column, DataType};
use switchy_database::DatabaseValue;

async fn create_schema(db: &dyn Database) -> Result<(), DatabaseError> {
    // Check if table exists first
    if !db.table_exists("users").await? {
        // Create table
        create_table("users")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "username".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(50),
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::VarChar(255),
                default: None,
            })
            .column(Column {
                name: "created_at".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: Some(DatabaseValue::Now),
            })
            .primary_key("id")
            .execute(db)
            .await?;
    }

    // Create index
    db.create_index("idx_users_email")
        .table("users")
        .column("email")
        .unique(true)
        .execute(db)
        .await?;

    Ok(())
}
```

### Schema Introspection

```rust
use switchy_database::{Database, DatabaseError};

async fn inspect_schema(db: &dyn Database) -> Result<(), DatabaseError> {
    // List all tables
    let tables = db.list_tables().await?;
    println!("Tables: {:?}", tables);

    // Check if a table exists
    if db.table_exists("users").await? {
        println!("Users table exists");
    }

    // Check if a column exists
    if db.column_exists("users", "email").await? {
        println!("Email column exists");
    }

    // Get complete table information
    if let Some(table_info) = db.get_table_info("users").await? {
        println!("Table: {}", table_info.name);

        // Inspect columns
        for (col_name, col_info) in &table_info.columns {
            println!("  Column: {} {:?} {}",
                col_name,
                col_info.data_type,
                if col_info.nullable { "NULL" } else { "NOT NULL" }
            );

            if col_info.is_primary_key {
                println!("    (Primary Key)");
            }
        }

        // Inspect indexes
        for (idx_name, idx_info) in &table_info.indexes {
            println!("  Index: {} on {:?} {}",
                idx_name,
                idx_info.columns,
                if idx_info.unique { "(UNIQUE)" } else { "" }
            );
        }

        // Inspect foreign keys
        for (fk_name, fk_info) in &table_info.foreign_keys {
            println!("  FK: {}.{} -> {}.{}",
                table_info.name, fk_info.column,
                fk_info.referenced_table, fk_info.referenced_column
            );
        }
    }

    // Get just the columns
    let columns = db.get_table_columns("users").await?;
    for column in columns {
        println!("Column: {} ({})", column.name,
                 if column.nullable { "NULL" } else { "NOT NULL" });
    }

    Ok(())
}
```

### Raw SQL Queries

```rust
use switchy_database::{Database, DatabaseError, DatabaseValue};

async fn raw_queries(db: &dyn Database) -> Result<(), DatabaseError> {
    // Raw query without parameters (string interpolation - use carefully!)
    let rows = db.query_raw("SELECT * FROM tracks WHERE artist = 'The Beatles'").await?;

    // Raw query with parameters (safe from SQL injection)
    // Note: Parameter syntax varies by backend:
    // - rusqlite: ? placeholders
    // - sqlx-sqlite: ? placeholders
    // - PostgreSQL (raw/sqlx): $1, $2 placeholders
    // - MySQL (sqlx): ? placeholders
    let params = vec![DatabaseValue::String("The Beatles".to_string())];
    let rows = db.query_raw_params("SELECT * FROM tracks WHERE artist = ?", &params).await?;

    // Raw execution (no results)
    db.exec_raw("CREATE INDEX idx_tracks_artist ON tracks(artist)").await?;

    // Raw execution with parameters
    let params = vec![
        DatabaseValue::String("Come Together".to_string()),
        DatabaseValue::String("The Beatles".to_string()),
    ];
    db.exec_raw_params("INSERT INTO tracks (title, artist) VALUES (?, ?)", &params).await?;

    Ok(())
}
```

## Feature Flags

The following feature flags are available in `Cargo.toml`:

### Backend Features

- `sqlite-rusqlite` - SQLite backend using rusqlite driver
- `sqlite-sqlx` - SQLite backend using sqlx driver
- `postgres-raw` - PostgreSQL backend using tokio-postgres
- `postgres-sqlx` - PostgreSQL backend using sqlx
- `mysql` / `mysql-sqlx` - MySQL backend using sqlx
- `turso` - Turso (libSQL) cloud database support

### Additional Features

- `schema` - Schema management and introspection (enabled by default)
- `cascade` - CASCADE deletion support for schema operations
- `auto-reverse` - Auto-reverse migration support
- `simulator` - Database simulator for testing
- `decimal` - Decimal type support (rust_decimal)
- `uuid` - UUID type support
- `api` - Actix-web integration for web APIs

### Placeholder Features

- `all-placeholders` - Support for all placeholder styles
- `placeholder-question-mark` - `?` placeholder support
- `placeholder-dollar-number` - `$1, $2` placeholder support
- `placeholder-at-number` - `@1, @2` placeholder support
- `placeholder-colon-number` - `:1, :2` placeholder support
- `placeholder-named-colon` - `:name` placeholder support

## Error Handling

```rust
use switchy_database::DatabaseError;

match db.select("tracks").where_eq("id", track_id).execute_first(db).await {
    Ok(Some(row)) => println!("Found track: {:?}", row),
    Ok(None) => println!("Track not found"),
    Err(DatabaseError::NoRow) => {
        println!("No row returned");
    },
    Err(DatabaseError::InvalidSchema(msg)) => {
        eprintln!("Schema error: {}", msg);
    },
    Err(DatabaseError::AlreadyInTransaction) => {
        eprintln!("Already in a transaction");
    },
    Err(DatabaseError::TransactionCommitted) => {
        eprintln!("Transaction already committed");
    },
    Err(DatabaseError::TransactionRolledBack) => {
        eprintln!("Transaction already rolled back");
    },
    Err(e) => {
        eprintln!("Database error: {}", e);
    }
}
```

### Backend-Specific Errors

Each backend has its own error variant:

- `DatabaseError::Rusqlite(rusqlite::RusqliteDatabaseError)` - rusqlite backend errors
- `DatabaseError::SqliteSqlx(sqlx::sqlite::SqlxDatabaseError)` - sqlx SQLite errors
- `DatabaseError::Postgres(postgres::postgres::PostgresDatabaseError)` - raw PostgreSQL errors
- `DatabaseError::PostgresSqlx(sqlx::postgres::SqlxDatabaseError)` - sqlx PostgreSQL errors
- `DatabaseError::MysqlSqlx(sqlx::mysql::SqlxDatabaseError)` - sqlx MySQL errors
- `DatabaseError::Turso(turso::TursoDatabaseError)` - Turso errors

## Data Types

The `DatabaseValue` enum supports the following types:

- **Strings**: `String`, `StringOpt`
- **Booleans**: `Bool`, `BoolOpt`
- **Integers**: `Int8`, `Int16`, `Int32`, `Int64` (and unsigned variants)
- **Floating Point**: `Real32`, `Real64`
- **Decimal**: `Decimal` (with `decimal` feature)
- **UUID**: `Uuid` (with `uuid` feature)
- **DateTime**: `DateTime`, `Now`, `NowPlus`
- **Null**: `Null`

The `schema::DataType` enum provides database-agnostic type definitions:

- `Text` - Variable-length text
- `VarChar(n)` - Fixed-length string
- `Bool` - Boolean
- `Int` - 32-bit integer
- `SmallInt` - 16-bit integer
- `BigInt` - 64-bit integer
- `Real` - 32-bit floating point
- `Double` - 64-bit floating point
- `DateTime` - Date and time
- `Decimal(precision, scale)` - Fixed-precision decimal

## Architecture

### Database Trait

The core `Database` trait provides:

- Query builder methods (`select`, `insert`, `update`, `delete`, `upsert`)
- Schema methods (`create_table`, `drop_table`, `create_index`, `alter_table`) - requires `schema` feature
- Execution methods (`query`, `query_first`, `exec_update`, `exec_insert`, etc.)
- Raw SQL methods (`query_raw`, `query_raw_params`, `exec_raw`, `exec_raw_params`)
- Introspection methods (`table_exists`, `column_exists`, `get_table_info`, `list_tables`) - requires `schema` feature
- Transaction method (`begin_transaction`)

### DatabaseTransaction Trait

The `DatabaseTransaction` trait extends `Database` with:

- `commit()` - Commit the transaction
- `rollback()` - Rollback the transaction
- `savepoint(name)` - Create a savepoint within the transaction
- CASCADE operations (with `cascade` feature)

### Savepoint Trait

The `Savepoint` trait provides:

- `release()` - Commit the savepoint
- `rollback_to()` - Rollback to the savepoint
- `name()` - Get the savepoint name

## Backend Implementation Details

### SQLite

Two SQLite implementations are available:

1. **rusqlite** (`sqlite-rusqlite` feature):
    - Uses `?` placeholders
    - Blocking operations wrapped in async
    - Connection pooling for concurrent transactions

2. **sqlx** (`sqlite-sqlx` feature):
    - Uses `?` placeholders
    - Native async support
    - Built-in connection pooling

### PostgreSQL

Two PostgreSQL implementations are available:

1. **Raw** (`postgres-raw` feature):
    - Uses tokio-postgres and deadpool-postgres
    - Uses `$1, $2` placeholders
    - Custom connection pool management

2. **sqlx** (`postgres-sqlx` feature):
    - Uses sqlx driver
    - Uses `$1, $2` placeholders
    - Built-in connection pooling

### MySQL

One MySQL implementation using sqlx:

- **sqlx** (`mysql-sqlx` feature):
    - Uses `?` placeholders (via transformation)
    - Built-in connection pooling
    - Full transaction support

### Turso

Turso (libSQL) cloud database support:

- Uses `?` placeholders
- Cloud-native database
- Compatible with SQLite API

## Limitations

- **No ORM**: This is a query builder, not a full ORM with automatic relationship mapping
- **No Migration System**: No built-in migration versioning or rollback system
- **Manual Schema Management**: Schema changes must be managed manually
- **No Query Optimization**: No automatic query analysis or optimization
- **Backend-Specific Placeholder Syntax**: Different backends require different placeholder styles (though some auto-transformation is provided)

## See Also

- [MoosicBox Config](../config/README.md) - Configuration management
- [MoosicBox Server](../server/README.md) - Server with database integration
