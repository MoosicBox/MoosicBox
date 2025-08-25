# Migration Guide: Updating Tests to Use MigrationTestBuilder

This guide helps you migrate existing tests from the old migration constants to the new `MigrationTestBuilder` pattern.

## Quick Migration Checklist

- [ ] Replace direct migration constant usage with `get_*_migrations()` functions
- [ ] Use `MigrationTestBuilder::new()` instead of calling migration methods directly
- [ ] Add `switchy_schema_test_utils` dependency to your test dependencies
- [ ] Specify custom table name `"__moosicbox_schema_migrations"` if needed
- [ ] Update timing of data insertion using `with_data_before/after` if applicable

## Before and After Examples

### Basic Migration Testing

**OLD (deprecated):**
```rust
// ❌ This pattern is no longer available
use moosicbox_schema::sqlite::SQLITE_LIBRARY_MIGRATIONS;

#[tokio::test]
async fn test_migrations() {
    let db = init_database().await;
    SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();
}
```

**NEW (recommended):**
```rust
// ✅ Use the new pattern
use moosicbox_schema::get_sqlite_library_migrations;
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_migrations() {
    let db = init_database().await;

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .run(&*db)
        .await
        .unwrap();
}
```

### Partial Migration Testing

**OLD (deprecated):**
```rust
// ❌ This pattern is no longer available
use moosicbox_schema::sqlite::SQLITE_LIBRARY_MIGRATIONS;

#[tokio::test]
async fn test_partial_migration() {
    let db = init_database().await;

    // Run migrations up to a specific point
    SQLITE_LIBRARY_MIGRATIONS.run_until(&*db, Some("2023-10-14-031701_create_tracks")).await.unwrap();

    // Insert test data
    db.exec_raw("INSERT INTO artists (title) VALUES ('Test')").await.unwrap();

    // Run remaining migrations
    SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();
}
```

**NEW (recommended):**
```rust
// ✅ Use the new pattern with better timing control
use moosicbox_schema::get_sqlite_library_migrations;
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_partial_migration() {
    let db = init_database().await;

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_data_after("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Insert test data after specific migration
            db.exec_raw("INSERT INTO artists (title) VALUES ('Test')").await?;
            Ok(())
        }))
        .run(&*db)
        .await
        .unwrap();
}
```

### Data Migration Testing

**OLD (complex, error-prone):**
```rust
// ❌ Manual migration management
use moosicbox_schema::sqlite::SQLITE_LIBRARY_MIGRATIONS;

#[tokio::test]
async fn test_data_migration() {
    let db = init_database().await;

    // Manually run migrations up to specific point
    for migration in SQLITE_LIBRARY_MIGRATIONS.migrations() {
        if migration.id() == "2023-10-14-031701_create_tracks" {
            break;
        }
        migration.up(&*db).await.unwrap();
    }

    // Insert data in old format
    db.exec_raw("INSERT INTO old_table (data) VALUES ('test')").await.unwrap();

    // Run the data migration
    // ... complex logic to find and run specific migration

    // Verify results
    let results = db.select("new_table").execute(&*db).await.unwrap();
    assert!(!results.is_empty());
}
```

**NEW (clean, declarative):**
```rust
// ✅ Clear, maintainable pattern
use moosicbox_schema::get_sqlite_library_migrations;
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_data_migration() {
    let db = init_database().await;

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_data_before("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Insert data in old format before migration runs
            db.exec_raw("INSERT INTO old_table (data) VALUES ('test')").await?;
            Ok(())
        }))
        .run(&*db)
        .await
        .unwrap();

    // Verify results - migrations have already run
    let results = db.select("new_table").execute(&*db).await.unwrap();
    assert!(!results.is_empty());
}
```

## Update Your Cargo.toml

Add the test utilities dependency to your `Cargo.toml`:

```toml
[dev-dependencies]
# Add this line
switchy_schema_test_utils = { workspace = true, features = ["sqlite"] }

# If you need PostgreSQL testing
switchy_schema_test_utils = { workspace = true, features = ["sqlite", "postgres"] }
```

## Available Migration Collection Functions

Replace old migration constants with these functions:

```rust
// SQLite migrations
use moosicbox_schema::{
    get_sqlite_library_migrations,  // Replaces: SQLITE_LIBRARY_MIGRATIONS
    get_sqlite_config_migrations,   // Replaces: SQLITE_CONFIG_MIGRATIONS
};

// PostgreSQL migrations
use moosicbox_schema::{
    get_postgres_library_migrations,  // Replaces: POSTGRES_LIBRARY_MIGRATIONS
    get_postgres_config_migrations,   // Replaces: POSTGRES_CONFIG_MIGRATIONS
};
```

## Common Migration Patterns

### 1. Simple Integration Test

```rust
#[tokio::test]
async fn test_schema_integration() {
    let db = switchy_database_connection::init_sqlite_sqlx(None).await.unwrap();

    // Run all migrations - they persist for integration testing
    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .run(&*db)
        .await
        .unwrap();

    // Your integration test code here
    // Database schema is now ready for testing
}
```

### 2. Migration Reversibility Test

```rust
#[tokio::test]
async fn test_migration_rollback() {
    let db = switchy_database_connection::init_sqlite_sqlx(None).await.unwrap();

    // Test that migrations can be rolled back cleanly
    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_rollback()  // This tests rollback functionality
        .run(&*db)
        .await
        .unwrap();

    // Database should be back to initial state
}
```

### 3. Complex Data Migration Test

```rust
#[tokio::test]
async fn test_complex_data_migration() {
    let db = switchy_database_connection::init_sqlite_sqlx(None).await.unwrap();

    MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
        .with_table_name("__moosicbox_schema_migrations")
        .with_initial_setup(|db| Box::pin(async move {
            // Set up initial database state
            db.exec_raw("INSERT INTO setup_table (id) VALUES (1)").await?;
            Ok(())
        }))
        .with_data_before("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Insert data before tracks migration
            db.exec_raw("INSERT INTO albums (title) VALUES ('Test Album')").await?;
            Ok(())
        }))
        .with_data_after("2023-10-14-031701_create_tracks", |db| Box::pin(async move {
            // Insert data after tracks migration - can now use track table
            db.exec_raw("INSERT INTO tracks (album_id, title) VALUES (1, 'Test Track')").await?;
            Ok(())
        }))
        .run(&*db)
        .await
        .unwrap();

    // Verify final state
    let tracks = db.select("tracks").execute(&*db).await.unwrap();
    assert_eq!(tracks.len(), 1);
}
```

## Benefits of the New Pattern

### ✅ Advantages

- **Cleaner API**: No direct access to migration internals
- **Better Testing**: Built-in rollback and state verification
- **Clear Timing**: Explicit `with_data_before`/`with_data_after` semantics
- **Error Safety**: Better error handling and recovery
- **Future-Proof**: Built on generic, reusable infrastructure

### ❌ Old Pattern Issues (Now Resolved)

- **Exposed Internals**: Direct access to migration constants
- **Manual Management**: Had to manually handle partial migrations
- **Timing Issues**: Unclear when to insert test data
- **Error Prone**: Easy to get migration ordering wrong
- **Hard to Maintain**: Tests tightly coupled to migration implementation

## Troubleshooting

### "column named id" Errors

If you see errors about missing `id` column, make sure you're using the correct table name:

```rust
// ✅ Correct - specify the table name moosicbox_schema uses
.with_table_name("__moosicbox_schema_migrations")

// ❌ Wrong - uses different table schema
.with_table_name("__switchy_migrations")  // This has different columns
```

### Migration Order Issues

The new system uses the same alphabetical ordering by migration ID as the old system. If you're seeing different behavior, verify your migration IDs are correctly formatted.

### Async Closure Syntax

The `with_data_before`/`with_data_after` callbacks require specific async syntax:

```rust
// ✅ Correct syntax
.with_data_before("migration_id", |db| Box::pin(async move {
    // Your async code here
    Ok(())
}))

// ❌ Wrong - missing Box::pin
.with_data_before("migration_id", |db| async move {
    Ok(())
})
```

## Need Help?

- Check the [switchy_schema examples](../switchy/schema/examples/) for complete working examples
- Review the [switchy_schema_test_utils documentation](../switchy/schema/test_utils/README.md)
- Look at existing updated tests in the `packages/scan/src/output.rs` file for real-world examples
