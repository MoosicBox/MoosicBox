# MoosicBox Schema

Database migration system for the MoosicBox ecosystem, providing automated SQL migration management for PostgreSQL and SQLite databases with version tracking and rollback support.

## Features

- **Database Migrations**: Run SQL migration files automatically
- **Version Tracking**: Track applied migrations in a dedicated table
- **Multi-Database Support**: Support for both PostgreSQL and SQLite
- **Migration Organization**: Organize migrations by library/config categories
- **Partial Migration**: Run migrations up to a specific version
- **Error Handling**: Basic error handling for migration failures

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_schema = "0.1.1"

# Enable database-specific features
moosicbox_schema = { version = "0.1.1", features = ["sqlite", "postgres"] }
```

## Usage

### Running Library Migrations

```rust
use moosicbox_schema::{migrate_library, MigrateError};
use switchy_database::Database;

#[tokio::main]
async fn main() -> Result<(), MigrateError> {
    // Initialize your database connection
    let db: Box<dyn Database> = /* your database initialization */;

    // Run all library migrations
    migrate_library(&*db).await?;

    println!("Library migrations completed successfully");
    Ok(())
}
```

### Running Config Migrations

```rust
use moosicbox_schema::migrate_config;

// Run configuration-related migrations
migrate_config(&*db).await?;
```

### Partial Migrations

```rust
use moosicbox_schema::migrate_library_until;

// Run migrations up to a specific version
migrate_library_until(&*db, Some("20231201_add_indexes")).await?;
```

### Migration Structure

Migrations are organized in a specific directory structure:

```
migrations/
├── sqlite/
│   ├── library/
│   │   ├── 20231201_initial_schema/
│   │   │   └── up.sql
│   │   └── 20231202_add_indexes/
│   │       └── up.sql
│   └── config/
│       └── 20231201_config_tables/
│           └── up.sql
└── postgres/
    ├── library/
    │   └── 20231201_initial_schema/
    │       └── up.sql
    └── config/
        └── 20231201_config_tables/
            └── up.sql
```

### Custom Migration Runner

```rust
use moosicbox_schema::Migrations;
use include_dir::{include_dir, Dir};

// Include your migration directory at compile time
static MY_MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");

// Create a migrations instance
let migrations = Migrations {
    directory: MY_MIGRATIONS,
};

// Run all migrations
migrations.run(&*db).await?;

// Or run up to a specific migration
migrations.run_until(&*db, Some("20231201_initial")).await?;
```

### Error Handling

```rust
use moosicbox_schema::MigrateError;

match migrate_library(&*db).await {
    Ok(()) => println!("Migrations completed successfully"),
    Err(MigrateError::Database(db_err)) => {
        eprintln!("Database error during migration: {}", db_err);
    }
}
```

## Migration Tracking

The system automatically creates a `__moosicbox_schema_migrations` table to track which migrations have been applied:

```sql
CREATE TABLE __moosicbox_schema_migrations (
    name TEXT NOT NULL PRIMARY KEY,
    executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

## Supported Databases

- **SQLite**: Via the `sqlite` feature flag
- **PostgreSQL**: Via the `postgres` feature flag

Each database type has its own set of migration files optimized for that specific database system.

## Dependencies

- `switchy_database`: Database abstraction layer
- `include_dir`: Compile-time directory inclusion for migration files
- `thiserror`: Error handling utilities

## Error Types

- `MigrateError`: Wraps database errors that occur during migration execution

The migration system ensures your database schema stays up-to-date and consistent across different environments.
