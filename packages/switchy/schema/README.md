# Switchy Schema

A comprehensive schema migration system for the Switchy database ecosystem, providing type-safe migrations with sophisticated lifetime management, state tracking, and recovery capabilities.

## Features

- **Three Discovery Methods**: Embedded (compile-time), Directory (runtime), and Code (programmatic)
- **Lifetime-Aware Architecture**: Support for both owned (`'static`) and borrowed (`'a`) data patterns
- **Type-Safe Query Builders**: Integration with `switchy_database` query builders via the `Executable` trait
- **Migration Runner**: Robust execution engine with rollback capabilities
- **State Tracking**: Comprehensive migration status tracking with failure recovery
- **CLI Tool**: Full-featured command-line interface for migration management
- **Recovery System**: Built-in mechanisms for handling failed migrations and dirty states
- **Feature-Gated**: Modular design with optional discovery methods
- **Async/Await**: Full async support for database operations
- **Auto-Reversible Migrations**: Automatic DOWN migration generation for safe operations (requires `auto-reverse` feature)

## Quick Start

### Using the CLI (Recommended)

Install the CLI tool:

```bash
cargo install --path packages/switchy/schema/cli
```

Create and run migrations:

```bash
# Create a new migration
switchy-migrate create add_user_table

# Check migration status
switchy-migrate status -d postgres://localhost/mydb

# Run pending migrations
switchy-migrate migrate -d postgres://localhost/mydb

# Check for failed migrations
switchy-migrate status --show-failed -d postgres://localhost/mydb
```

### Library Usage

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

## Auto-Reversible Migrations

The `auto-reverse` feature enables automatic generation of DOWN migrations for safe operations. This reduces boilerplate and ensures consistency between UP and DOWN migrations.

### Enabling the Feature

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_schema = { version = "0.1.0", features = ["auto-reverse"] }
switchy_database = { version = "0.1.4", features = ["schema", "auto-reverse"] }
```

### Usage

Use `ReversibleCodeMigration` to automatically generate DOWN migrations:

```rust
use switchy_schema::discovery::code::ReversibleCodeMigration;
use switchy_database::schema::{create_table, Column, DataType};

let create = create_table("users")
    .column(Column {
        name: "id".to_string(),
        data_type: DataType::Int,
        nullable: false,
        auto_increment: true,
        default: None,
    })
    .primary_key("id");

// Automatically generates DROP TABLE as the DOWN migration
let migration = ReversibleCodeMigration::new("001_create_users", create).into();
```

### Supported Operations

The following operations support automatic reversal:

- **CREATE TABLE** → DROP TABLE
- **CREATE INDEX** → DROP INDEX
- **ADD COLUMN** → DROP COLUMN

### Type Safety

The type system prevents using non-reversible operations:

```rust
// This will NOT compile - DropTableStatement doesn't implement AutoReversible
let drop = drop_table("users");
let migration = ReversibleCodeMigration::new("bad", drop); // Compile error!
```

### Example

Run the complete example:

```bash
cargo run --example auto_reverse --features auto-reverse
```

See `examples/auto_reverse.rs` for a full working demonstration.

## CLI Tool

The `switchy-migrate` CLI provides comprehensive migration management capabilities.

### Installation

```bash
# Install from source
cargo install --path packages/switchy/schema/cli

# Or build locally
cd packages/switchy/schema/cli
cargo build --release
```

### Core Commands

#### Migration Management

```bash
# Create a new migration file
switchy-migrate create migration_name

# Run pending migrations
switchy-migrate migrate -d DATABASE_URL

# Check migration status
switchy-migrate status -d DATABASE_URL

# Rollback migrations
switchy-migrate rollback -d DATABASE_URL --steps 1
```

#### Recovery Commands

```bash
# Show failed migrations
switchy-migrate status --show-failed -d DATABASE_URL

# Retry a failed migration
switchy-migrate retry MIGRATION_ID -d DATABASE_URL

# Mark a migration as completed (use with caution)
switchy-migrate mark-completed MIGRATION_ID -d DATABASE_URL

# Mark untracked migrations as completed (default, safest)
switchy-migrate mark-all-completed -d DATABASE_URL

# Also mark failed migrations as completed
switchy-migrate mark-all-completed --include-failed -d DATABASE_URL

# Also mark in-progress migrations as completed
switchy-migrate mark-all-completed --include-in-progress -d DATABASE_URL

# Mark ALL migrations as completed (most dangerous)
switchy-migrate mark-all-completed --all -d DATABASE_URL

# Force without confirmation
switchy-migrate mark-all-completed --all --force -d DATABASE_URL

# Force migration past dirty state (dangerous)
switchy-migrate migrate --force -d DATABASE_URL
```

### Environment Variables

Set these to avoid repeating common options:

```bash
export SWITCHY_DATABASE_URL="postgres://localhost/mydb"
export SWITCHY_MIGRATIONS_DIR="./migrations"
export SWITCHY_MIGRATION_TABLE="__switchy_migrations"
```

### Migration Marking Scopes

When using `mark-all-completed`, you can control which migration states are affected:

#### Default Behavior (Safest)

```bash
switchy-migrate mark-all-completed -d DATABASE_URL
```

Only marks untracked (pending) migrations as completed:

| Current State | Action  | Result     |
| ------------- | ------- | ---------- |
| Not Tracked   | ✅ Mark | Completed  |
| Completed     | ⏭️ Skip | Completed  |
| Failed        | ⏭️ Skip | Failed     |
| InProgress    | ⏭️ Skip | InProgress |

**Use When:** Initializing tracking for an existing database

#### With `--include-failed`

```bash
switchy-migrate mark-all-completed --include-failed -d DATABASE_URL
```

Marks pending and failed migrations:

| Current State | Action    | Result     |
| ------------- | --------- | ---------- |
| Not Tracked   | ✅ Mark   | Completed  |
| Completed     | ⏭️ Skip   | Completed  |
| Failed        | ⚠️ Update | Completed  |
| InProgress    | ⏭️ Skip   | InProgress |

**Use When:** Multiple failed migrations were manually fixed

#### With `--include-in-progress`

```bash
switchy-migrate mark-all-completed --include-in-progress -d DATABASE_URL
```

Marks pending and in-progress migrations:

| Current State | Action    | Result    |
| ------------- | --------- | --------- |
| Not Tracked   | ✅ Mark   | Completed |
| Completed     | ⏭️ Skip   | Completed |
| Failed        | ⏭️ Skip   | Failed    |
| InProgress    | ⚠️ Update | Completed |

**Use When:** Migration process was interrupted/crashed

#### With `--all` (Most Dangerous)

```bash
switchy-migrate mark-all-completed --all -d DATABASE_URL
```

Marks all migrations regardless of state:

| Current State | Action    | Result    |
| ------------- | --------- | --------- |
| Not Tracked   | ✅ Mark   | Completed |
| Completed     | ⏭️ Skip   | Completed |
| Failed        | ⚠️ Update | Completed |
| InProgress    | ⚠️ Update | Completed |

**Use When:** Complete reset/sync of migration tracking table

#### With `--drop` (CRITICAL - DESTROYS HISTORY)

```bash
switchy-migrate mark-all-completed --drop -d DATABASE_URL
```

**CRITICAL OPERATION** - Drops the entire migration tracking table before marking:

**Steps:**

1. 🗑️ DROP entire `__switchy_migrations` table
2. 🆕 CREATE fresh tracking table with current schema
3. ✅ MARK all source migrations as completed with new checksums

**What You Lose:**

- All migration execution history
- Timestamps of when migrations ran
- Failure reasons and error messages
- Old checksums for validation
- All status tracking (completed/failed/in-progress)

**Use When:**

- ✅ Migration tracking table is corrupted or unreadable
- ✅ Table schema is incompatible with current code
- ✅ Need to completely reset migration history
- ❌ **NOT** for normal recovery - use scopes instead

**Example:**

```bash
# Most common: drop and recreate with default scope (pending only)
switchy-migrate mark-all-completed --drop -d DATABASE_URL

# With scope control (marks even failed migrations after drop)
switchy-migrate mark-all-completed --drop --include-failed -d DATABASE_URL
```

⚠️ **WARNING**: Each scope level increases danger. The `--drop` flag is CRITICAL level. Always backup before using flags.

### Migration States

The system tracks four migration states:

- **Pending**: Migration not yet executed
- **In Progress**: Migration currently running (may indicate crash)
- **Completed**: Migration executed successfully
- **Failed**: Migration failed with recorded error

### Recovery Workflows

#### Failed Migration Recovery

```bash
# 1. Check what failed
switchy-migrate status --show-failed

# 2. Fix the migration file if needed
# 3. Retry the migration
switchy-migrate retry 001_create_users

# Or mark as completed if manually fixed
switchy-migrate mark-completed 001_create_users --force
```

#### Dirty State Recovery

```bash
# Check for in-progress migrations
switchy-migrate status --show-failed

# Force past dirty state if safe
switchy-migrate migrate --force
```

## Migration Runner

The `MigrationRunner` provides programmatic migration execution:

```rust
use switchy_schema::runner::MigrationRunner;
use switchy_database::Database;

let runner = MigrationRunner::new(Box::new(source))
    .with_table_name("__switchy_migrations");

// Run all pending migrations
runner.run(db).await?;

// Check migration status
let info = runner.list_migrations(db).await?;
for migration in info {
    println!("Migration {}: {:?}", migration.id, migration.status);
}

// Retry a specific migration
runner.retry_migration(db, "001_create_users").await?;

// Mark untracked migrations as completed (default, safest)
let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::PendingOnly).await?;
println!("Marked {} new migrations", summary.newly_marked);

// Mark failed migrations as completed too
let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::IncludeFailed).await?;
println!("Marked {} failed migrations", summary.failed_marked);

// Mark ALL migrations as completed (dangerous)
let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::All).await?;
println!("Total: {}, New: {}, Failed: {}, InProgress: {}",
         summary.total, summary.newly_marked, summary.failed_marked, summary.in_progress_marked);
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

## Migration Table Schema

The system creates a `__switchy_migrations` table to track migration state:

| Column           | Type        | Description                           |
| ---------------- | ----------- | ------------------------------------- |
| `id`             | TEXT        | Unique migration identifier           |
| `run_on`         | DATETIME    | When migration started                |
| `finished_on`    | DATETIME    | When migration completed              |
| `up_checksum`    | VARCHAR(64) | Content hash for up migration         |
| `down_checksum`  | VARCHAR(64) | Content hash for down migration       |
| `status`         | TEXT        | Current state (pending/completed/etc) |
| `failure_reason` | TEXT        | Error message if failed               |

## Error Handling & Recovery

The package provides comprehensive error handling with recovery capabilities:

```rust
use switchy_schema::{Result, MigrationError};

// Handle different error types
match runner.run(db).await {
    Ok(()) => println!("Migrations completed successfully"),
    Err(MigrationError::DirtyState { migrations }) => {
        println!("Migrations in progress - may need recovery: {:?}", migrations);
        // Use runner.retry_migration(db, &id) or CLI recovery commands
    }
    Err(MigrationError::ChecksumValidationFailed { mismatches }) => {
        println!("Migration content changed after execution: {:?}", mismatches);
        // Investigate migration file changes
    }
    Err(MigrationError::Database(db_err)) => {
        println!("Database error: {}", db_err);
        // Handle database connectivity or SQL errors
    }
    Err(err) => println!("Other error: {}", err),
}
```

### MigrationError Types

The system provides structured migration errors:

- **DirtyState**: Migrations currently running (in-progress state)
- **ChecksumValidationFailed**: Migration file(s) changed after execution
- **Database**: Database connectivity or SQL errors
- **Execution**: Migration execution failures
- **Discovery**: Migration discovery failures
- **Validation**: Migration validation failures

## Features

The package supports optional features for different capabilities:

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
- `auto-reverse` - Auto-reversible migrations support
- `validation` - Migration validation utilities
- `test-utils` - Testing utilities

## Best Practices

### Development Workflow

1. **Always check status before migrating**:

    ```bash
    switchy-migrate status --show-failed
    ```

2. **Use dry-run for testing**:

    ```bash
    switchy-migrate migrate --dry-run
    ```

3. **Monitor long-running migrations** - Check status during execution

4. **Handle dirty states properly** - Don't force unless you understand the implications

### Migration Design

5. **Use `'static` for most cases** - This covers the vast majority of migration scenarios

6. **Prefer embedded migrations for libraries** - They're self-contained and don't require external files

7. **Use directory migrations for applications** - They're easier to manage and update

8. **Use code migrations for dynamic scenarios** - When migrations need to be generated programmatically

9. **Leverage query builders** - They provide type safety and database abstraction

10. **Include rollback migrations** - Always provide `down.sql` when possible

### Safety & Recovery

11. **Use transactions for data migrations** - Ensures atomicity

12. **Test migrations on copies of production data** - Catch issues before deployment

13. **Keep migrations idempotent when possible** - Safe to re-run

14. **Document complex migrations** - Include comments explaining business logic

15. **Have a recovery plan** - Know how to handle failures in production

## Troubleshooting

### Common Issues and Solutions

#### Migration Shows as "In Progress"

**Cause**: Process crashed or was interrupted during migration.

**Solution**:

```bash
# Check status
switchy-migrate status --show-failed

# If migration actually failed, retry it
switchy-migrate retry MIGRATION_ID

# If migration completed but wasn't marked, mark as completed
switchy-migrate mark-completed MIGRATION_ID --force
```

#### Checksum Mismatch Error

**Cause**: Migration file was modified after being executed.

**Solution**:

1. Determine if the change was intentional
2. If unintentional, revert the migration file
3. If intentional, create a new migration with the changes

#### Database Connection Failures

**Cause**: Network issues, wrong credentials, or database unavailable.

**Solution**:

```bash
# Test connection
switchy-migrate status -d DATABASE_URL

# Check environment variables
echo $SWITCHY_DATABASE_URL

# Verify database is running and accessible
```

#### Force Migration Warnings

**Cause**: Using `--force` flag bypasses safety checks.

**Understanding**:

- Only use `--force` when you understand the implications
- Can cause data loss or corruption if used incorrectly
- Always backup before using `--force` in production

## Architecture Overview

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   CLI Tool      │────▶│  Migration       │────▶│   Database      │
│  (switchy-      │     │  Runner          │     │   Connection    │
│   migrate)      │     └──────────────────┘     └─────────────────┘
└─────────────────┘              │                         │
                                 ▼                         ▼
                    ┌──────────────────┐     ┌─────────────────┐
                    │   Discovery      │     │  Migrations     │
                    │   Sources        │     │  Table          │
                    │ ┌──────────────┐ │     │ (__switchy_     │
                    │ │  Embedded    │ │     │  migrations)    │
                    │ │  Directory   │ │     └─────────────────┘
                    │ │  Code        │ │
                    │ └──────────────┘ │
                    └──────────────────┘
```

## Examples

### CLI Usage Examples

```bash
# Basic workflow
switchy-migrate create add_users_table
switchy-migrate migrate -d postgres://localhost/mydb
switchy-migrate status -d postgres://localhost/mydb

# Recovery workflow
switchy-migrate status --show-failed -d postgres://localhost/mydb
switchy-migrate retry 001_add_users_table -d postgres://localhost/mydb

# Advanced usage
switchy-migrate migrate --dry-run -d postgres://localhost/mydb
switchy-migrate rollback --steps 2 -d postgres://localhost/mydb
switchy-migrate mark-completed 002_add_indexes -d postgres://localhost/mydb --force

# Mark untracked migrations as completed (safest)
switchy-migrate mark-all-completed -d postgres://localhost/mydb

# Mark failed migrations too (recovery scenario)
switchy-migrate mark-all-completed --include-failed -d postgres://localhost/mydb

# Mark in-progress migrations too (crash recovery)
switchy-migrate mark-all-completed --include-in-progress -d postgres://localhost/mydb

# Mark ALL migrations (initialization/force sync)
switchy-migrate mark-all-completed --all --force -d postgres://localhost/mydb
```

### Library Examples

See the `examples/` directory for complete working examples:

#### Running Examples

Both examples are full Cargo projects with proper dependencies:

```bash
# Static migrations example (most common patterns)
cd examples/static_migrations
cargo run

# Borrowed migrations example (advanced lifetime patterns)
cd examples/borrowed_migrations
cargo run
```

#### Example Projects

- **`examples/static_migrations/`** - Complete project demonstrating:
    - Custom migration implementations with `'static` lifetimes
    - All three discovery methods (embedded, directory, code)
    - Query builder integration
    - Migration runner usage
    - Comprehensive test coverage

- **`examples/borrowed_migrations/`** - Advanced project showing:
    - Configuration-driven migrations with borrowed data
    - Explicit lifetime management (`'a`)
    - Temporary migration sources
    - Function-based migration generation

### Integration Example

```rust
use switchy_schema::{
    runner::MigrationRunner,
    discovery::directory::DirectoryMigrationSource,
};
use switchy_database::Database;

async fn run_migrations(db: &dyn Database) -> switchy_schema::Result<()> {
    let source = DirectoryMigrationSource::from_path("./migrations");
    let runner = MigrationRunner::new(Box::new(source))
        .with_table_name("__switchy_migrations");

    // Check current status
    let migrations = runner.list_migrations(db).await?;
    for migration in &migrations {
        println!("Migration {}: {:?}", migration.id, migration.status);
    }

    // Run pending migrations
    runner.run(db).await?;
    println!("Migrations completed successfully");

    Ok(())
}
```

## Documentation

For more detailed information:

- **[CLI Recovery Commands](../../../spec/generic-schema-migrations/recovery-commands.md)** - Complete CLI command reference
- **[Migration System Specification](../../../spec/generic-schema-migrations/plan.md)** - Technical architecture and design
- **API Documentation** - Run `cargo doc --open` for full API reference

## License

This project is licensed under the same terms as the parent Switchy project.
