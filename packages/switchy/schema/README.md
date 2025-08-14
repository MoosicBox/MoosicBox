# Switchy Schema

A generic schema migration system for the Switchy database ecosystem, providing type-safe migrations with sophisticated lifetime management.

## Features

- **Three Discovery Methods**: Embedded (compile-time), Directory (runtime), and Code (programmatic)
- **Lifetime-Aware Architecture**: Support for both owned (`'static`) and borrowed (`'a`) data patterns
- **Type-Safe Query Builders**: Integration with `switchy_database` query builders via the `Executable` trait
- **Feature-Gated**: Modular design with optional discovery methods
- **Async/Await**: Full async support for database operations

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_schema = { version = "0.1.0", features = ["all-discovery"] }
switchy_database = { version = "0.1.4", features = ["schema"] }
```

### Basic Usage (Static Migrations)

Most migrations use the `'static` lifetime and own their data:

```rust
use switchy_schema::migration::{Migration, MigrationSource};
use switchy_database::Database;
use async_trait::async_trait;

struct MyMigration {
    id: String,
    sql: String,
}

#[async_trait]
impl Migration<'static> for MyMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw(&self.sql).await?;
        Ok(())
    }
}
```

## Discovery Methods

### 1. Embedded Migrations (Compile-Time)

Embed migration files directly into your binary:

```rust
use switchy_schema::discovery::embedded::EmbeddedMigrationSource;
use include_dir::{include_dir, Dir};

static MIGRATIONS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

let source = EmbeddedMigrationSource::new(&MIGRATIONS_DIR);
let migrations = source.migrations().await?;
```

**Directory Structure:**
```
migrations/
├── 001_create_users/
│   ├── up.sql
│   └── down.sql
└── 002_add_indexes/
    └── up.sql
```

### 2. Directory Migrations (Runtime)

Load migrations from the filesystem at runtime:

```rust
use switchy_schema::discovery::directory::DirectoryMigrationSource;
use std::path::PathBuf;

let source = DirectoryMigrationSource::from_path(PathBuf::from("./migrations"));
let migrations = source.migrations().await?;
```

### 3. Code Migrations (Programmatic)

Define migrations programmatically using raw SQL or query builders:

```rust
use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
use switchy_database::schema::{create_table, Column, DataType};

// Raw SQL migration
let sql_migration = CodeMigration::new(
    "001_create_users".to_string(),
    Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string()),
    Some(Box::new("DROP TABLE users".to_string())),
);

// Query builder migration
let builder_migration = CodeMigration::new(
    "002_create_posts".to_string(),
    Box::new(
        create_table("posts")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id")
    ),
    None,
);

let mut source = CodeMigrationSource::new();
source.add_migration(sql_migration);
source.add_migration(builder_migration);
```

## Lifetime Architecture

### Static Lifetime Pattern (`'static`)

**Use for:** Migrations that own all their data (99% of use cases)

```rust
// Embedded migrations - always 'static
let embedded_source = EmbeddedMigrationSource::new(&MIGRATIONS_DIR);

// Directory migrations - always 'static
let directory_source = DirectoryMigrationSource::from_path(path);

// Code migrations with owned data
let owned_migration = CodeMigration::new(
    "001_test".to_string(),
    Box::new("CREATE TABLE test (id INTEGER)".to_string()),
    None,
);
```

### Non-Static Lifetime Pattern (`'a`)

**Use for:** Advanced scenarios with borrowed data

```rust
use switchy_database::schema::{create_table, Column, DataType};

fn create_table_migration<'a>(table_name: &'a str) -> CodeMigration<'a> {
    let stmt = create_table(table_name)
        .column(Column {
            name: "id".to_string(),
            nullable: false,
            auto_increment: true,
            data_type: DataType::Int,
            default: None,
        })
        .primary_key("id");

    CodeMigration::new(
        format!("create_{}", table_name),
        Box::new(stmt),
        None,
    )
}

// Usage with borrowed data
let migration = create_table_migration("products");
```

## Migration Guide

### Updating from Non-Lifetime Version

If you have existing code using the old API, add lifetime annotations:

```rust
// Old API
impl Migration for MyMigration { ... }
impl MigrationSource for MySource { ... }

// New API
impl Migration<'static> for MyMigration { ... }
impl MigrationSource<'static> for MySource { ... }
```

### Type Annotations

When working with migration collections:

```rust
// Static migrations
let migrations: Vec<Box<dyn Migration<'static> + 'static>> = source.migrations().await?;

// Borrowed migrations (advanced)
let migrations: Vec<Box<dyn Migration<'a> + 'a>> = source.migrations().await?;
```

## Features

The package supports optional features for different discovery methods:

```toml
[dependencies]
switchy_schema = { version = "0.1.0", features = ["embedded", "directory", "code"] }

# Or use all discovery methods
switchy_schema = { version = "0.1.0", features = ["all-discovery"] }

# Minimal (just core traits)
switchy_schema = { version = "0.1.0", default-features = false }
```

Available features:
- `embedded` - Compile-time embedded migrations
- `directory` - Runtime directory-based migrations
- `code` - Programmatic code-based migrations
- `all-discovery` - All discovery methods
- `validation` - Migration validation utilities
- `test-utils` - Testing utilities

## Best Practices

1. **Use `'static` for most cases** - This covers the vast majority of migration scenarios
2. **Prefer embedded migrations for libraries** - They're self-contained and don't require external files
3. **Use directory migrations for applications** - They're easier to manage and update
4. **Use code migrations for dynamic scenarios** - When migrations need to be generated programmatically
5. **Leverage query builders** - They provide type safety and database abstraction

## Error Handling

The package provides comprehensive error types:

```rust
use switchy_schema::{Result, Error};

match source.migrations().await {
    Ok(migrations) => {
        // Process migrations
    }
    Err(Error::Database(db_err)) => {
        // Handle database errors
    }
    Err(Error::Io(io_err)) => {
        // Handle I/O errors (directory discovery)
    }
    Err(Error::Discovery(msg)) => {
        // Handle discovery errors
    }
    Err(Error::Validation(msg)) => {
        // Handle validation errors
    }
}
```

## Examples

See the `examples/` directory for complete working examples:

- `static_migrations.rs` - Basic usage with static lifetimes
- `borrowed_migrations.rs` - Advanced usage with borrowed data

## License

This project is licensed under the same terms as the parent Switchy project.
