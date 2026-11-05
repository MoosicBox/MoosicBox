# Switchy Database

Database abstraction layer with support for multiple database backends, schema management, and transactions.

## Overview

The Switchy Database package provides:

- **Multi-Database Support**: SQLite (rusqlite and sqlx), PostgreSQL (raw and sqlx), MySQL (sqlx), DuckDB, Turso, remote libSQL servers, ClickHouse, SQL Server, and Oracle
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
- **DuckDB**: Embedded analytical database using duckdb driver with `?` placeholders
- **Turso**: Turso (local libSQL/SQLite-compatible) database support
- **libSQL (remote)**: `sqld`, `turso dev` and Turso Cloud databases over Hrana over HTTP, with `?` placeholders
- **ClickHouse**: ClickHouse servers over the HTTP interface, with `$n` placeholders sent as typed query parameters
- **SQL Server**: Microsoft SQL Server and Azure SQL over TDS (`tiberius`), with `@P1`, `@P2`, ... placeholders
- **Oracle**: Oracle Database 12c and later through ODPI-C (the `oracle` crate, which loads Oracle Instant Client at runtime), with `:1`, `:2`, ... placeholders
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
use switchy_database::{Database, DatabaseError, query::FilterableQuery};

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
| SQLite     | ✅ Full            | Can create savepoints after errors  |
| PostgreSQL | ✅ Full            | Must create before potential errors |
| MySQL      | ✅ Full (InnoDB)   | Requires InnoDB storage engine      |
| DuckDB     | ❌ Not supported   | Returns `UnsupportedOperation`      |

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

#### Schemas, Views and Other Relations

The table-oriented methods above use the connection's default schema. The
relation methods work across schemas and include views, materialized views
and foreign tables:

```rust
use switchy_database::{Database, DatabaseError, schema::RelationRef};

async fn browse(db: &dyn Database) -> Result<(), DatabaseError> {
    for schema in db.list_schemas().await? {
        if schema.is_system {
            continue;
        }
        for relation in db.list_relations(Some(&schema.name)).await? {
            println!(
                "{}.{} [{}] ~{:?} rows",
                schema.name, relation.name, relation.kind, relation.estimated_rows
            );
            if let Some(info) = db.get_relation_info(&relation.relation_ref()).await? {
                for column in info.columns.values() {
                    // `native_type` is the database's exact type name
                    println!("  {} {}", column.name, column.native_type);
                }
            }
            if relation.kind.is_view() {
                let sql = db.get_view_definition(&relation.relation_ref()).await?;
                println!("  definition: {sql:?}");
            }
        }
    }
    let _ = RelationRef::with_schema("public", "users");
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
    // - DuckDB: ? placeholders
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

    // Positional result set with column metadata (names + native type names),
    // available even when no rows are returned; duplicate names are preserved
    let result = db.query_raw_result("SELECT 1 AS a, 2 AS a", &[]).await?;
    for column in &result.columns {
        println!("{} {:?}", column.name, column.type_name);
    }

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
- `duckdb` - DuckDB embedded analytical database
- `duckdb-bundled` - DuckDB with bundled library (no system install required)
- `turso` - Turso local database support (file-based or in-memory), without selecting a global allocator
- `turso-mimalloc` - Explicitly enables `turso` and Turso's mimalloc global allocator; not enabled by defaults or `turso`
- `libsql` - Remote libSQL servers (`sqld`, `turso dev`, Turso Cloud) over Hrana over HTTP (`reqwest` with rustls)
- `clickhouse` - ClickHouse servers over the HTTP interface (`reqwest` with rustls)
- `mssql` - Microsoft SQL Server over TDS (`tiberius` with rustls; named instances through SQL Browser)
- `oracle` - Oracle Database through ODPI-C (the `oracle` crate; Oracle Instant Client must be installed where the program runs, and is loaded on the first connection)

#### Optional Turso allocator

Applications can opt in with:

```toml
switchy_database = { version = "0.4.0", default-features = false, features = ["turso-mimalloc"] }
```

This forwards to `turso/mimalloc`, which installs a Rust global allocator for the
**entire linked artifact**, not only database allocations. Do not also declare a
different global allocator in that artifact. Cargo features are additive: a
transitive dependency enabling this feature enables the allocator for other
consumers sharing that artifact too.

Keep this feature disabled in reusable libraries and independently linked native
plugins. Multiple mimalloc instances can collide through fixed thread-local slots
on macOS, causing memory corruption. Executable-owned allocator selection is
preferred when an application loads plugins. Performance benefits are workload-
and platform-dependent; benchmark before enabling it.

### Additional Features

- `schema` - Schema management and introspection (enabled by default)
- `cascade` - CASCADE deletion support for schema operations
- `auto-reverse` - Auto-reverse migration support
- `simulator` - Database simulator for testing
- `decimal` - Decimal type support (rust_decimal)
- `uuid` - UUID type support
- `json` - JSON value support (`DatabaseValue::Json`, serde_json)
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
use switchy_database::{Database, DatabaseError, query::FilterableQuery};

// Within an async function with access to db: &dyn Database and track_id
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
- `DatabaseError::DuckDb(duckdb::DuckDbDatabaseError)` - DuckDB errors
- `DatabaseError::Turso(turso::TursoDatabaseError)` - Turso errors
- `DatabaseError::Libsql(libsql::LibsqlDatabaseError)` - remote libSQL errors
- `DatabaseError::ClickHouse(clickhouse::ClickHouseDatabaseError)` - ClickHouse errors
- `DatabaseError::Mssql(mssql::MssqlDatabaseError)` - SQL Server errors (server errors keep their number, state, class, line and procedure)
- `DatabaseError::Oracle(oracle::OracleDatabaseError)` - Oracle errors (server errors keep their `ORA-` code and offset; a missing Instant Client is `ClientLibrary`)

## Data Types

The `DatabaseValue` enum supports the following types:

- **Strings**: `String`, `StringOpt`
- **Booleans**: `Bool`, `BoolOpt`
- **Integers**: `Int8`, `Int16`, `Int32`, `Int64` (and unsigned variants)
- **Floating Point**: `Real32`, `Real64`
- **Decimal**: `Decimal` (with `decimal` feature)
- **UUID**: `Uuid` (with `uuid` feature)
- **DateTime**: `DateTime`, `Now`, `NowPlus`
- **Date/Time**: `Date`, `Time`, `DateTimeTz` (offset-aware) and their `*Opt` variants
- **Binary**: `Bytes`, `BytesOpt`
- **JSON**: `Json`, `JsonOpt` (with `json` feature)
- **Fallback**: `Other { type_name, text }` for native types without a dedicated variant
  (row decoding never fails on unknown column types)
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
- Raw SQL methods (`query_raw`, `query_raw_params`, `query_raw_result`, `exec_raw`, `exec_raw_params`)
- Introspection methods (`table_exists`, `column_exists`, `get_table_info`, `list_tables`,
  `list_schemas`, `list_databases`, `list_relations`, `get_relation_info`, `get_view_definition`) - requires `schema` feature
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

### Global and Profile Database Access

The crate also exposes public APIs for resolving `Database` instances in applications:

- `config::init(database)` - Register a global `Arc<Box<dyn Database>>` singleton
- `config::ConfigDatabase` - Wrapper that dereferences to `dyn Database` (and supports Actix extraction with `api` feature)
- `profiles::PROFILES` - Global profile registry for multiple named databases
- `profiles::LibraryDatabase` - Profile-resolved wrapper that dereferences to `dyn Database` (and supports Actix extraction with `api` feature)

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

Turso (libSQL/SQLite-compatible) local database support:

- Uses `?` placeholders
- Local file-based and in-memory database support
- Compatible with SQLite API

### libSQL (remote)

Remote libSQL servers (`libsql` feature): `sqld`, `turso dev` and Turso Cloud.

- `LibsqlDatabase::builder("libsql://db-org.turso.io").auth_token(token).build()`;
  `libsql://` means `https://`, and `http://` URLs (a local `sqld`) skip TLS
- Each statement outside a transaction runs on its own server stream, closed in
  the same HTTP request; a transaction holds one stream from `BEGIN` to
  `COMMIT`/`ROLLBACK` and pings it in the background, since servers close idle
  streams after about ten seconds
- Introspection reads the SQLite catalog (`PRAGMA table_xinfo`, `index_list`,
  `foreign_key_list`); `ALTER TABLE` adds, drops and renames columns (changing
  a column needs a table rebuild and is refused)
- Read-only mode (`.read_only(true)`): libSQL servers refuse `PRAGMA query_only`,
  so the server describes each statement first, in a request of its own, and a
  statement it does not report read-only is refused without ever being sent to
  run; one that passes runs in a transaction (or savepoint) that is always
  rolled back (one extra round trip per statement)

### ClickHouse

ClickHouse servers (`clickhouse` feature), over the HTTP interface.

- `ClickHouseDatabase::builder("http://127.0.0.1:8123").user(user).password(password).database("shop").build()`;
  `https://` URLs use TLS with the system's trusted certificates; the password
  travels in the `X-ClickHouse-Key` header
- Raw SQL and introspection only: `$1`, `$2`, ... placeholders become typed
  substitutions (`{p1:String}`) whose values are sent as `param_p1` HTTP
  parameters, never written into the statement (`?` is ClickHouse's ternary
  operator); the query and schema builders return
  `DatabaseError::UnsupportedOperation` (ClickHouse has no `RETURNING`, changes
  rows through asynchronous mutations, and needs a table engine)
- Results come back as `TabSeparatedWithNamesAndTypes` and decode by column
  type (`Nullable`/`LowCardinality` looked through; `Array`, `Map`, `Tuple`,
  enums and big integers stay text); a statement asking for another `FORMAT`
  returns its output as text rows
- No transactions: `begin_transaction` returns `UnsupportedOperation`
- Introspection reads `system.databases` (as schemas), `system.tables`,
  `system.columns`, `system.data_skipping_indices` and `system.functions`; the
  primary key is reported as a non-unique index, since it doesn't make rows
  unique, and there are no foreign keys
- Read-only mode (`.read_only(true)`): `readonly=1`, ClickHouse's own read-only
  mode, is sent as the last parameter of every request; settings given with
  `.setting(name, value)` may not name it (or any parameter the backend sets)

### SQL Server

Microsoft SQL Server 2016 and later, Azure SQL Database and Azure SQL Edge
(`mssql` feature), over TDS with SQL Server logins.

- `MssqlDatabase::builder("db.example.com").database("shop").user(user).password(password).build()`;
  `.port()`, `.instance()` (a named instance, found through SQL Browser),
  `.encryption()` (`Plaintext`, `LoginOnly`, `Preferred`, `Required` by
  default, or TDS 8.0 `Strict`), `.trust_server_certificate()`, `.ca_file()`,
  `.host_name_in_certificate()`, `.application_name()`, `.pool_size()`,
  `.connect_timeout()` and `.statement_timeout()` (a statement that runs over
  closes its connection, so the server rolls it back)
- Raw SQL takes `@P1`, `@P2`, ... placeholders, sent through `sp_executesql`
  as typed parameters; `DatabaseValue::Now` becomes `SYSUTCDATETIME()`
- The query builders generate T-SQL (`TOP`, `OUTPUT INSERTED.*` /
  `DELETED.*` for returned rows, `MERGE … WITH (HOLDLOCK)` for upserts), and
  the schema builders `IDENTITY` columns, `NVARCHAR`, `DATETIME2`, and
  `sp_rename` renames (dropping a column first drops its default constraint)
- Values decode by type: `money` and `decimal`/`numeric` exactly (as
  `Decimal`, or text beyond its range), `datetime` rounded to milliseconds as
  SQL Server shows it, `datetimeoffset` with its offset, `uniqueidentifier`
  as a UUID, `xml` and `sql_variant` as text
- Transactions with savepoints (`SAVE TRANSACTION`); if the server rolls a
  transaction back itself (a deadlock, or an error under `XACT_ABORT`), the
  transaction reports `MssqlDatabaseError::TransactionLost`. In autocommit,
  a statement that leaves a transaction open is rolled back and reported
  (`OpenTransaction`)
- Introspection reads the `sys` catalog: schemas, databases the login can
  open, tables and views with `MS_Description` comments and row estimates,
  columns (alias types, computed and identity columns, parsed defaults),
  indexes, foreign keys, view definitions, and functions and procedures
- Read-only mode (`.read_only(true)`): SQL Server has no read-only session
  setting, so the backend enforces it. A conservative T-SQL tokenizer check
  admits only single queries (`SELECT`, `WITH`, `(`) free of words that
  write, run code, change the session or leave a transaction, and no
  `NEXT VALUE FOR` or four-part (linked-server) names; `EXEC` only of a few
  catalog procedures (`mssql::READ_ONLY_PROCEDURES`, unqualified or
  `sys.`-qualified) with literal arguments.
  A statement that passes runs in `BEGIN TRANSACTION` (connections set
  `XACT_ABORT ON`) and is rolled back. Afterwards `@@TRANCOUNT` must still be
  1, otherwise it returns `ReadOnlyViolation` and the connection is closed, as
  it is on any error, timeout or cancellation. That check only *detects* a
  statement that ended the transaction: a commit it made has already taken
  effect, so preventing writes rests on the statement check. CLR functions,
  and T-SQL functions calling extended stored procedures, can still act
  outside the transaction when a query calls them. For a guarantee from the
  server itself, use a login with only `db_datareader`

### Oracle

Oracle Database 12c and later (`oracle` feature), with database users and
passwords, through ODPI-C. Oracle Instant Client is loaded when the first
connection opens; without it, connecting returns
`OracleDatabaseError::ClientLibrary` (DPI-1047).

- `OracleDatabase::builder("db.example.com").service_name("FREEPDB1").user(user).password(password).build()`;
  `.port()`, `.sid()` (an instance's SID instead of a service),
  `OracleDatabaseBuilder::connect_string()` (a net service name, Easy
  Connect string or connect descriptor, given to the client as it is),
  `.tls()` (TCPS), `.wallet_location()`, `.server_cert_dn()`, `.pool_size()`,
  `.connect_timeout()` and `.statement_timeout()` (the client's call
  timeout). Host, service and wallet values that would change the connect
  descriptor's shape are refused
- The driver is synchronous: calls run on Tokio's blocking pool, and a
  dropped (cancelled) call breaks the running statement
- Raw SQL takes `:1`, `:2`, ... placeholders, bound as typed values (text
  as `VARCHAR2`/`CLOB`, never `NVARCHAR2`); `DatabaseValue::Now` becomes
  `SYS_EXTRACT_UTC(SYSTIMESTAMP)`. Sessions use ISO date and timestamp
  formats
- The query builders generate Oracle SQL (`FETCH FIRST n ROWS ONLY`,
  `RETURNING ROWID INTO` and a re-read for returned rows, deletes by
  `ROWID`, `MERGE` for upserts), and the schema builders identity columns,
  `VARCHAR2`/`CLOB`/`NUMBER`, and `IF [NOT] EXISTS` as PL/SQL blocks ignoring
  the one error the object's presence (or absence) raises, so they work
  before 23ai too
- Values decode by type: `NUMBER` exactly (as `Decimal` up to 28 digits,
  text beyond), `DATE`/`TIMESTAMP` to the fraction, `TIMESTAMP WITH TIME
  ZONE` with its offset, `TIMESTAMP WITH LOCAL TIME ZONE` in the session's
  zone, `RAW`/`BLOB` as bytes, `CLOB` as text, 23ai `BOOLEAN`; intervals,
  `ROWID` and other types as text. Native `JSON` columns can't be fetched by
  the driver: the error suggests `JSON_SERIALIZE`
- Transactions with savepoints; Oracle commits an open transaction around
  every DDL statement
- Introspection reads the `ALL_*` views: schemas (users), tables, views and
  materialized views with comments and row estimates, columns (identity
  columns, defaults), indexes, foreign keys and view definitions
- Read-only mode (`.read_only(true)`) is enforced by the backend in three
  steps. A conservative tokenizer check (`oracle::READ_ONLY_DENIED_WORDS`,
  `oracle::READ_ONLY_DENIED_PREFIXES`) admits only single queries
  (`SELECT`, `WITH`, `(`) free of words that write, run code, change the
  session or end a transaction, sequence `NEXTVAL`/`CURRVAL`, supplied
  packages that act outside the query (`DBMS_*`, `UTL_*`, ...) and database
  links (`@`), whose parenthesized calls are all unqualified built-in
  functions (`oracle::READ_ONLY_FUNCTIONS`). A keyword before `(` passes
  only if Oracle reserves it (`oracle::READ_ONLY_RESERVED_PAREN_WORDS`) or,
  for an unreserved one that could name a function
  (`oracle::READ_ONLY_CONTEXT_PAREN_KEYWORDS`), only where the grammar makes
  it a keyword (`OVER (` after `)`, `PIVOT (` after a table). On 23ai and later, `ALTER
  SESSION SET READ_ONLY = TRUE` is re-asserted before every statement;
  before 23ai each statement runs in `SET TRANSACTION READ ONLY`. Every
  statement is rolled back afterwards. A read-only transaction holds one
  connection but runs each statement alone the same way (Oracle can't
  re-assert the session mode inside an open transaction). A user function
  called without parentheses (`SELECT f FROM dual`) or from a view can't be
  seen by the check; on 23ai the session mode still refuses its writes
  unless it switches that mode off itself. For a guarantee from the server
  itself, use a 23ai `READ ONLY` user, or one with only `SELECT` grants

### DuckDB

Embedded analytical database:

- **DuckDB** (`duckdb` / `duckdb-bundled` feature):
    - Uses `?` placeholders
    - Connection pool of 5 connections behind `Arc<Mutex<>>`
    - Supports routing/consistency config (`DuckDbConfig`):
        - `Deterministic` mode (default): one shared connection topology for predictable behavior
        - `Pooled` mode: round-robin across independent connections
        - `Strict` consistency (default): serialized operation gate in pooled mode
        - `Relaxed` consistency: higher throughput, weaker cross-connection guarantees
    - Blocking operations wrapped in async
    - In-memory and file-backed databases supported
    - For cross-connection transaction behavior, prefer file-backed databases in tests
    - `NOW()` is cast to `TIMESTAMP` (DuckDB's `NOW()` returns `TIMESTAMP WITH TIME ZONE`)
    - Auto-increment uses `CREATE SEQUENCE` + `DEFAULT nextval(...)` instead of `GENERATED ALWAYS AS IDENTITY`
    - Savepoints are not supported (returns `UnsupportedOperation`)
    - Schema introspection via `information_schema` and `duckdb_indexes()`
    - `DELETE ... RETURNING` uses a SELECT-then-DELETE workaround (see source for details)
    - `DROP TABLE ... CASCADE` for FK-dependent tables is limited in DuckDB v1.4.4
    - `ALTER TABLE ... DROP COLUMN` with index/FK dependencies is limited in DuckDB v1.4.4

## Limitations

- **No ORM**: This is a query builder, not a full ORM with automatic relationship mapping
- **No Migration System**: No built-in migration versioning or rollback system
- **Manual Schema Management**: Schema changes must be managed manually
- **No Query Optimization**: No automatic query analysis or optimization
- **Backend-Specific Placeholder Syntax**: Different backends require different placeholder styles (though some auto-transformation is provided)

## See Also

- [MoosicBox Config](../../config/README.md) - Configuration management
- [MoosicBox Server](../../server/README.md) - Server with database integration
