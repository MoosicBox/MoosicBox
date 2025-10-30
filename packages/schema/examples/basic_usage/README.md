# Basic Database Migration Usage Example

This example demonstrates the fundamental workflow for using `moosicbox_schema` to manage database migrations in a MoosicBox application.

## Summary

A complete, runnable example showing how to initialize a SQLite database, execute configuration and library migrations, and verify the migration status. This demonstrates the most common use case for `moosicbox_schema` in production applications.

## What This Example Demonstrates

- Initializing a SQLite database connection using `switchy_database_connection`
- Running configuration migrations with `migrate_config()`
- Running library migrations with `migrate_library()`
- Proper error handling for migration operations
- Querying the migration tracking table to verify results
- Displaying migration status and history

## Prerequisites

- Basic understanding of database migrations
- Familiarity with async Rust (tokio)
- SQLite installed (for sqlx runtime compilation, if applicable)

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/schema/examples/basic_usage/Cargo.toml
```

Or with verbose logging to see detailed migration progress:

```bash
RUST_LOG=debug cargo run --manifest-path packages/schema/examples/basic_usage/Cargo.toml
```

## Expected Output

```
Starting MoosicBox Schema Migration Example
===========================================

Step 1: Initializing SQLite database connection...
✓ Database connection established

Step 2: Running configuration migrations...
✓ Configuration migrations completed successfully

Step 3: Running library migrations...
✓ Library migrations completed successfully

Step 4: Verifying migrations...
Found 47 tracked migrations:
  - Completed: 47
  - Failed: 0
  - In Progress: 0

Most recent migrations:
  ✓ 2025-05-31-110603_update_api_source_id_structure [completed]
  ✓ 2024-12-19-142019_add_album_versions_table [completed]
  ✓ 2024-10-27-082547_create_api_sources [completed]
  ✓ 2024-09-21-130720_set_journal_mode_to_wal [completed]
  ✓ 2024-08-25-031422_create_tracks_index [completed]

===========================================
Migration example completed successfully!
```

The exact number of migrations and their names will depend on the current state of the MoosicBox schema.

## Code Walkthrough

### Database Initialization

```rust
let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
```

This creates an in-memory SQLite database connection using sqlx. For production use, you would pass a path:

```rust
let db = switchy_database_connection::init_sqlite_sqlx(Some("/path/to/database.db")).await?;
```

### Running Configuration Migrations

```rust
migrate_config(&*db).await?;
```

Configuration migrations set up system-level tables and settings that the MoosicBox application needs to function. These migrations are typically run once during initial setup.

### Running Library Migrations

```rust
migrate_library(&*db).await?;
```

Library migrations create and modify the music library schema (artists, albums, tracks, etc.). These are the core data tables for MoosicBox's functionality.

### Error Handling Pattern

```rust
match migrate_library(&*db).await {
    Ok(()) => println!("✓ Library migrations completed successfully"),
    Err(e) => {
        eprintln!("✗ Library migrations failed: {e}");
        return Err(e.into());
    }
}
```

Always handle migration errors explicitly, as failed migrations can leave the database in an inconsistent state. The `MigrateError` type provides detailed information about what went wrong.

### Verifying Migration Status

The example queries the `__moosicbox_schema_migrations` tracking table to show:

- Total number of migrations
- Migration status breakdown (completed, failed, in-progress)
- Most recent migrations with their status

This verification step is useful for debugging and confirming that migrations executed successfully.

## Key Concepts

### Migration Tracking Table

Every migration is recorded in the `__moosicbox_schema_migrations` table with:

- **id**: Migration name (e.g., `2024-10-27-082547_create_api_sources`)
- **status**: One of `completed`, `failed`, or `in_progress`
- **run_on**: Timestamp when migration started
- **finished_on**: Timestamp when migration completed
- **up_checksum**: SHA-256 hash of the migration SQL
- **down_checksum**: SHA-256 hash of the rollback SQL

### Embedded Migrations

Migrations are compiled into the binary at build time from the `packages/schema/migrations/` directory. This means:

- No external migration files needed at runtime
- Zero-config deployment
- Guaranteed version consistency between code and schema

### Idempotent Execution

The migration system is idempotent - running migrations multiple times is safe:

- Already-completed migrations are skipped
- Only new migrations are executed
- Failed migrations can be retried after fixing the issue

### Multi-Database Support

While this example uses SQLite, the same code works with PostgreSQL by enabling the `postgres` feature and using an appropriate database connection:

```rust
let db = switchy_database_connection::init_postgres("postgresql://...").await?;
```

Both `migrate_config()` and `migrate_library()` automatically run the correct migrations for each database type when multiple features are enabled.

## Testing the Example

### Verify In-Memory Database

This example uses an in-memory SQLite database, so the data is lost when the program exits. You can verify this by running the example twice and noting that migrations are always executed (since the database is fresh each time).

### Test with Persistent Database

Modify the code to use a file-based database:

```rust
let db = switchy_database_connection::init_sqlite_sqlx(Some("./test.db")).await?;
```

Run the example twice:

1. First run: All migrations execute
2. Second run: Migrations are skipped (already completed)

Clean up afterwards:

```bash
rm test.db
```

### Test Error Handling

You can simulate migration failures by:

1. Corrupting a migration file in `packages/schema/migrations/`
2. Using an invalid database path
3. Setting incorrect database permissions

## Troubleshooting

### "migrations table already exists" Error

**Symptom**: Error about `__moosicbox_schema_migrations` table already existing

**Solution**: This typically happens in development when the migration tracking gets out of sync. Options:

1. Delete the database file and start fresh (for SQLite)
2. Use the environment variable to rebuild tracking:

```bash
MOOSICBOX_DROP_MIGRATIONS_TABLE=1 cargo run --manifest-path packages/schema/examples/basic_usage/Cargo.toml
```

### Migration Checksum Mismatch

**Symptom**: Error about checksum validation failing

**Solution**: A migration file was modified after being applied. This is a safety check to prevent schema inconsistency. Either:

1. Revert the migration file to its original content
2. Create a new migration with the intended changes
3. Drop and rebuild the database (development only)

### SQLite "database is locked" Error

**Symptom**: Database locked errors during migration

**Solution**: Ensure no other processes are accessing the database file. SQLite uses file-level locking, so only one writer can access the database at a time.

## Related Examples

- **Partial Migration Example** (planned): Demonstrates using `migrate_library_until()` to run migrations up to a specific version
- **Migration Testing Example** (planned): Shows how to test migrations using `MigrationTestBuilder`
- **Environment Control Example** (planned): Demonstrates using `MOOSICBOX_SKIP_MIGRATION_EXECUTION` for deployment scenarios

## Further Reading

- [moosicbox_schema README](../../README.md) - Complete package documentation
- [switchy_schema](../../../switchy/schema/) - Underlying migration engine
- [switchy_database](../../../switchy/database/) - Database abstraction layer
