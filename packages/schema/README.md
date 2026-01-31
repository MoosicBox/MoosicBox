# MoosicBox Schema

Database migration system for the MoosicBox ecosystem. This package provides a thin wrapper around [switchy_schema](../switchy/schema) for automated SQL migration management with PostgreSQL and SQLite databases, featuring version tracking and comprehensive testing utilities.

## Features

- **Database Migrations**: Automated SQL migration execution with rollback support
- **Version Tracking**: Track applied migrations in `__moosicbox_schema_migrations` table
- **Multi-Database Support**: Both PostgreSQL and SQLite via switchy_database
- **Environment Control**: Skip migrations via `MOOSICBOX_SKIP_MIGRATION_EXECUTION=1`
- **Partial Migrations**: Run migrations up to a specific version
- **Comprehensive Testing**: Advanced test utilities for migration validation
- **Built on switchy_schema**: Modern, generic migration engine

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_schema = { workspace = true }

# Enable database-specific features as needed
moosicbox_schema = { workspace = true, features = ["sqlite", "postgres"] }
```

## Usage

### Running Library Migrations

```rust
use moosicbox_schema::{migrate_library, MigrateError};
use switchy_database::Database;

#[tokio::main]
async fn main() -> Result<(), MigrateError> {
    // Initialize your database connection
    // (use your preferred database connection method)
    let db: Box<dyn Database> = todo!("Initialize your database connection");

    // Run all library migrations
    migrate_library(&*db).await?;

    println!("Library migrations completed successfully");
    Ok(())
}
```

### Running Config Migrations

```rust
use moosicbox_schema::migrate_config;

// Initialize your database connection
let db: Box<dyn Database> = todo!("Initialize your database connection");

// Run configuration-related migrations
migrate_config(&*db).await?;
```

### Partial Migrations

```rust
use moosicbox_schema::migrate_library_until;

// Initialize your database connection
let db: Box<dyn Database> = todo!("Initialize your database connection");

// Run migrations up to a specific version
migrate_library_until(&*db, Some("2023-10-14-031701_create_tracks")).await?;
```

### Environment Variables

#### Drop Migration Tracking Table

```bash
export MOOSICBOX_DROP_MIGRATIONS_TABLE=1
```

When this environment variable is set to "1", the migration tracking table (`__moosicbox_schema_migrations`) will be dropped before running migrations. This removes all migration history.

**Warning:** This is a destructive operation. Use only for:

- **Fresh setup**: Resetting a development database
- **Testing**: Creating clean test environments
- **Recovery**: Rebuilding corrupted tracking tables

This is typically followed by running migrations with `MOOSICBOX_SKIP_MIGRATION_EXECUTION=1` to rebuild the tracking table.

#### Skip Migration Execution

```bash
export MOOSICBOX_SKIP_MIGRATION_EXECUTION=1
```

When this environment variable is set to "1", migration functions will populate
the migration tracking table (`__moosicbox_schema_migrations`) with all migrations
marked as completed WITHOUT executing their SQL. This is useful for:

- **Initialization**: Setting up tracking for existing databases with matching schema
- **Read-only deployments**: Applications that shouldn't modify schema
- **Recovery**: When schema table needs to be rebuilt after corruption

**Behavior:**

- ✅ Creates the migration tracking table if it doesn't exist
- ✅ Records all migrations as completed with timestamps
- ✅ Logs summary of marked migrations (newly marked, updated, already completed)
- ❌ Does NOT execute any migration SQL

**Previous Behavior:** Completely skipped all migration operations. The new behavior
ensures proper migration state tracking even when SQL execution is skipped.

**Example:**

```rust
use moosicbox_schema::migrate_library;

// Set environment variable
std::env::set_var("MOOSICBOX_SKIP_MIGRATION_EXECUTION", "1");

// Initialize your database connection
let db: Box<dyn Database> = todo!("Initialize your database connection");

// This will populate the table without executing SQL
migrate_library(&*db).await?;
// Logs: "marked 45 migrations as completed (45 newly marked, 0 failed skipped, 0 in-progress skipped)"
```

**Scope Behavior:**

The `MOOSICBOX_SKIP_MIGRATION_EXECUTION` environment variable uses the safest scope (`PendingOnly`):

- ✅ Only marks untracked migrations as completed
- ⏭️ Preserves failed migration states (they remain failed)
- ⏭️ Preserves in-progress migration states (they remain in-progress)

This ensures that if you have failed migrations tracked, they won't be incorrectly marked as completed. The environment variable is designed for initialization scenarios, not recovery from failed migrations.

**If you need to mark failed migrations as completed**, use the CLI instead:

```bash
switchy-migrate mark-all-completed --include-failed -d DATABASE_URL
```

### Migration Testing

For testing migrations, use the provided test utilities:

```rust
use moosicbox_schema::{get_sqlite_library_migrations};
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_library_migrations() {
    // Initialize test database connection
    let db: Box<dyn Database> = todo!("Initialize test database connection");
    let migrations = get_sqlite_library_migrations().await.unwrap();

    // Run all migrations and verify they work
    MigrationTestBuilder::new(migrations)
        .with_table_name("__moosicbox_schema_migrations")
        .run(&*db)
        .await
        .unwrap();
}
```

For more advanced migration testing patterns, see the [Migration Testing Guide](#migration-testing-guide) section below.

### Error Handling

```rust
use moosicbox_schema::MigrateError;

// Initialize your database connection
let db: Box<dyn Database> = todo!("Initialize your database connection");

match migrate_library(&*db).await {
    Ok(()) => println!("Migrations completed successfully"),
    Err(MigrateError::Database(db_err)) => {
        eprintln!("Database error during migration: {}", db_err);
    }
    Err(MigrateError::Schema(schema_err)) => {
        eprintln!("Schema migration error: {}", schema_err);
    }
}
```

## Architecture

This package is built on top of [switchy_schema](../switchy/schema), providing MoosicBox-specific migration management while leveraging the generic, reusable migration engine. The architecture includes:

- **Embedded Migrations**: SQL files compiled into the binary at build time
- **Automatic Discovery**: Migrations loaded from `/migrations/server/{library,config}/{sqlite,postgres}/` directories
- **Version Tracking**: Uses `__moosicbox_schema_migrations` table (customized from default `__switchy_migrations`)
- **Environment Integration**: Respects `MOOSICBOX_SKIP_MIGRATION_EXECUTION` environment variable

## Migration Structure

Migrations are organized in the following directory structure:

```
migrations/server/
├── library/
│   ├── sqlite/
│   │   ├── 2023-10-13-195407_create_artists/
│   │   │   ├── up.sql
│   │   │   └── down.sql
│   │   └── 2023-10-14-031701_create_tracks/
│   │       ├── up.sql
│   │       └── down.sql
│   └── postgres/
│       ├── 2023-10-13-195407_create_artists/
│       │   ├── up.sql
│       │   └── down.sql
│       └── 2023-10-14-031701_create_tracks/
│           ├── up.sql
│           └── down.sql
└── config/
    ├── sqlite/
    └── postgres/
```

## Migration Testing Guide

### Basic Migration Testing

Use `MigrationTestBuilder` for comprehensive migration validation:

```rust
use moosicbox_schema::get_sqlite_library_migrations;
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_all_migrations() {
    // Initialize test database connection
    let db: Box<dyn Database> = todo!("Initialize test database connection");

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .run(&*db)
        .await
        .unwrap();
}
```

### Data Migration Testing

Test migrations that transform existing data:

```rust
#[tokio::test]
async fn test_data_migration() {
    // Initialize test database connection
    let db: Box<dyn Database> = todo!("Initialize test database connection");

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_data_before("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Insert test data in old format
            db.exec_raw("INSERT INTO artists (title) VALUES ('Test Artist')").await?;
            Ok(())
        }))
        .with_data_after("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Verify data is preserved and new structure works
            let result = db.select("artists").columns(&["id", "title"]).execute(db).await?;
            assert_eq!(result.len(), 1);
            Ok(())
        }))
        .run(&*db)
        .await
        .unwrap();
}
```

### Testing with Rollback

Test migration rollback functionality:

```rust
#[tokio::test]
async fn test_migration_rollback() {
    // Initialize test database connection
    let db: Box<dyn Database> = todo!("Initialize test database connection");

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_rollback() // Enable rollback testing
        .run(&*db)
        .await
        .unwrap();

    // Verify database is back to initial state
}
```

### Available Test Collection Functions

```rust
// Get migrations for testing - all return Vec<Arc<dyn Migration>>
use moosicbox_schema::{
    get_sqlite_library_migrations,
    get_sqlite_config_migrations,
    get_postgres_library_migrations,
    get_postgres_config_migrations,
};
```

## Supported Databases

- **SQLite**: Via the `sqlite` feature flag
- **PostgreSQL**: Via the `postgres` feature flag

Both databases use the same migration table schema but have database-specific SQL in their migration files.

## Migration Tracking

The system automatically creates a `__moosicbox_schema_migrations` table to track applied migrations:

```sql
CREATE TABLE __moosicbox_schema_migrations (
    id TEXT NOT NULL,
    run_on DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_on DATETIME,
    up_checksum VARCHAR(64) NOT NULL,
    down_checksum VARCHAR(64) NOT NULL,
    status TEXT NOT NULL,
    failure_reason TEXT
);
```

The tracking table records:

- **id**: Unique migration identifier (e.g., `2023-10-13-195407_create_artists`)
- **run_on**: When the migration started executing
- **finished_on**: When the migration completed (NULL if still in progress)
- **up_checksum**: SHA-256 checksum of the up migration SQL
- **down_checksum**: SHA-256 checksum of the down migration SQL
- **status**: Migration status (`completed`, `failed`, or `in_progress`)
- **failure_reason**: Error message if migration failed (NULL if successful)

## Dependencies

- `switchy_schema`: Generic migration engine
- `switchy_database`: Database abstraction layer
- `switchy_env`: Environment variable handling
- `include_dir`: Compile-time directory inclusion
- `thiserror`: Error handling utilities

## Error Types

- `MigrateError`: Wraps both database and schema migration errors
    - `MigrateError::Database(DatabaseError)`: Database connection/execution errors
    - `MigrateError::Schema(SwitchyMigrationError)`: Migration logic errors

## Development Notes

- Migrations are embedded at compile time for zero-config deployment
- The `build.rs` script ensures recompilation when migration files change
- Both PostgreSQL and SQLite migrations run when both features are enabled (intended for development/testing)
- In production deployments, typically only one database feature is enabled
