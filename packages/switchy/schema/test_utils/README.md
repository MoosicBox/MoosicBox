# Switchy Schema Test Utils

Test utilities for `switchy_schema` that provide comprehensive migration testing capabilities.

## Overview

This package provides testing utilities for validating database migrations:

- `verify_migrations_full_cycle` - Basic up/down migration testing
- `verify_migrations_with_state` - Migration testing with pre-seeded state
- `verify_migrations_with_mutations` - Testing with data mutations between migration steps
- `MigrationTestBuilder` - Builder pattern for complex migration scenarios with breakpoints (requires `sqlite` feature)
- `MigrationSnapshotTest` - Schema snapshot comparison testing (requires `snapshots` feature)
- `assertions` module - Database schema and state assertion helpers (requires `sqlite` feature)
- `create_empty_in_memory` - Create an in-memory SQLite database for testing (requires `sqlite` feature)

## Features

- **Database Support**: SQLite support via feature flags (additional databases planned)
- **Comprehensive Testing**: Full migration lifecycle validation
- **State Preservation**: Verify data integrity during migrations
- **Mutation Testing**: Test against various database states and scenarios
- **Async Support**: Full async/await support for modern Rust applications
- **Snapshot Testing**: Schema snapshot comparison (optional `snapshots` feature)

## Usage

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
switchy_schema_test_utils = { workspace = true, features = ["sqlite"] }
```

### Creating a Test Database

Use `create_empty_in_memory` to create an in-memory SQLite database for testing (requires `sqlite` feature):

```rust
use switchy_schema_test_utils::create_empty_in_memory;

#[tokio::test]
async fn test_example() {
    let db = create_empty_in_memory().await.unwrap();
    // Use db for migration testing...
}
```

### Basic Migration Testing

```rust
use switchy_schema_test_utils::verify_migrations_full_cycle;

#[tokio::test]
async fn test_migrations() {
    let db = /* your database connection */;
    let migrations = vec![/* your migrations */];
    verify_migrations_full_cycle(db, migrations).await.unwrap();
}
```

### State Preservation Testing

```rust
use switchy_schema_test_utils::verify_migrations_with_state;

#[tokio::test]
async fn test_migrations_with_data() {
    let migrations = vec![/* your migrations */];
    verify_migrations_with_state(
        db,
        migrations,
        |db| Box::pin(async move {
            // Setup initial state before migrations
            db.exec_raw("INSERT INTO existing_table (id) VALUES (1)").await?;
            Ok(())
        })
    ).await.unwrap();
}
```

### Mutation Testing

```rust
use switchy_schema_test_utils::verify_migrations_with_mutations;
use switchy_schema_test_utils::mutations::MutationBuilder;

#[tokio::test]
async fn test_migrations_comprehensive() {
    let db = /* your database connection */;
    let migrations = vec![/* your migrations */];
    let mutations = MutationBuilder::new()
        .add_mutation("001_create_users", "INSERT INTO users (name) VALUES ('test')")
        .build();
    verify_migrations_with_mutations(
        db,
        migrations,
        mutations,
    ).await.unwrap();
}
```

### Advanced: Migration Test Builder (requires `sqlite` feature)

```rust
use switchy_schema_test_utils::MigrationTestBuilder;

#[tokio::test]
async fn test_data_migration() {
    let db = /* your database connection */;
    let migrations = vec![/* your migrations */];

    MigrationTestBuilder::new(migrations)
        .with_data_before("002_migrate_data", |db| {
            Box::pin(async move {
                // Insert test data before the migration runs
                db.exec_raw("INSERT INTO old_table (value) VALUES ('test')").await?;
                Ok(())
            })
        })
        .with_data_after("002_migrate_data", |db| {
            Box::pin(async move {
                // Verify migration transformed data correctly
                db.exec_raw("SELECT * FROM new_table WHERE value = 'test'").await?;
                Ok(())
            })
        })
        .run(db)
        .await
        .unwrap();
}
```

### Schema and State Assertions (requires `sqlite` feature)

```rust
use switchy_schema_test_utils::assertions::*;

#[tokio::test]
async fn test_migration_schema() {
    let db = /* your database connection */;

    // Verify table existence
    assert_table_exists(db, "users").await.unwrap();
    assert_table_not_exists(db, "old_table").await.unwrap();

    // Verify columns
    assert_column_exists(db, "users", "email", "TEXT").await.unwrap();

    // Verify data state
    assert_row_count(db, "users", 5).await.unwrap();
    assert_row_count_min(db, "posts", 10).await.unwrap();

    // Verify migrations applied
    assert_migrations_applied(db, &["001_initial", "002_add_users"]).await.unwrap();
}
```

### Snapshot Testing (requires `snapshots` feature)

```rust
use switchy_schema_test_utils::MigrationSnapshotTest;

#[tokio::test]
async fn test_migration_snapshots() {
    MigrationSnapshotTest::new("my_migration_test")
        .migrations_dir("migrations")
        .expected_tables(vec!["users".to_string(), "posts".to_string()])
        .assert_schema(true)
        .assert_sequence(true)
        .run()
        .await
        .unwrap();
}
```

## Cargo Features

- `sqlite` - Enable SQLite support (included in default features)
- `snapshots` - Enable schema snapshot testing capabilities
- `decimal` - Enable decimal type support (included in default features)
- `uuid` - Enable UUID type support (included in default features)
- `fail-on-warnings` - Treat warnings as errors during compilation

## Examples

See the `examples/` directory in the parent `switchy_schema` package for complete working examples of each testing utility.
