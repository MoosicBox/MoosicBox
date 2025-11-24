# Basic Usage Example

This example demonstrates the core functionality of `switchy_schema` using type-safe schema builders instead of raw SQL.

## What This Example Shows

### Clean, Type-Safe Migrations

- **Create Table**: Uses `db.create_table()` with `Column` structs for type safety
- **Create Index**: Uses `db.create_index()` with fluent builder API
- **Alter Table**: Uses `db.alter_table().add_column()` for schema evolution
- **Drop Operations**: Uses `db.drop_table()` and `db.drop_index()` for rollback

### Complete Migration Lifecycle

- **Migration Status**: Check which migrations are applied/pending
- **Forward Migration**: Run all pending migrations
- **Schema Verification**: Insert and query test data to verify schema
- **Rollback Support**: Clean `down()` methods for all migrations (commented example)

## Key Features Demonstrated

1. **Zero Raw SQL**: All operations use type-safe builders
2. **Migration Tracking**: Automatic tracking in custom table (`__example_migrations`)
3. **Error Handling**: Proper error propagation with `MigrationError`
4. **Database Abstraction**: Uses `switchy_database` for database-agnostic operations (demonstrated with SQLite)

## Prerequisites

- Rust 1.70 or later
- Basic understanding of database schemas and migrations
- Familiarity with async Rust (`async`/`await`)

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/basic_usage
cargo run
```

## Expected Output

```
🚀 Starting Basic Usage Example
================================

📋 Checking migration status...
  001_create_users_table - Create users table with id, name, and email columns ❌ Pending
  002_add_email_index - Add index on email column for faster lookups ❌ Pending
  003_add_created_at_column - Add created_at timestamp column to track when users are created ❌ Pending

🔧 Running migrations...
✅ All migrations completed successfully!

🧪 Verifying schema with test data...
📝 Inserted user with ID: Row { ... }
👤 User: 1 - Alice Johnson (created: alice@example.com)

📊 Final migration status:
  001_create_users_table - Create users table with id, name, and email columns ✅ Applied
  002_add_email_index - Add index on email column for faster lookups ✅ Applied
  003_add_created_at_column - Add created_at timestamp column to track when users are created ✅ Applied

🎉 Basic usage example completed successfully!
```

## Migration Patterns

### 1. Create Table Migration

```rust
async fn up(&self, db: &dyn Database) -> Result<(), MigrationError> {
    db.create_table("users")
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column { /* ... */ })
        .primary_key("id")
        .execute(db)
        .await?;
    Ok(())
}
```

### 2. Create Index Migration

```rust
async fn up(&self, db: &dyn Database) -> Result<(), MigrationError> {
    db.create_index("idx_users_email")
        .table("users")
        .column("email")
        .if_not_exists(true)
        .execute(db)
        .await?;
    Ok(())
}
```

### 3. Alter Table Migration

```rust
async fn up(&self, db: &dyn Database) -> Result<(), MigrationError> {
    db.alter_table("users")
        .add_column(
            "created_at".to_string(),
            DataType::DateTime,
            false,
            Some(DatabaseValue::Now)
        )
        .execute(db)
        .await?;
    Ok(())
}
```

## Architecture

- **Code-Based Migrations**: Migrations defined as Rust structs implementing `Migration` trait
- **Custom Migration Source**: `BasicUsageMigrations` implements `MigrationSource` trait
- **Type Safety**: All schema operations use strongly-typed builders
- **Database Agnostic**: Uses `switchy_database` abstraction layer

## Rollback Support

Uncomment the rollback section at the end of `main()` to see rollback in action:

```rust
runner.rollback(db, switchy_schema::runner::RollbackStrategy::Steps(1)).await?;
```

## Testing the Example

The example runs against an in-memory SQLite database, so each run starts fresh. You can verify:

1. **Migration tracking**: Check that migrations transition from ❌ Pending to ✅ Applied
2. **Data insertion**: Verify that the user data is successfully inserted and queried
3. **Schema evolution**: Observe that the `created_at` column is added in the third migration

## Troubleshooting

**Issue**: `error: no bin target named 'basic_usage'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

**Issue**: Migrations appear to run but no output is shown

- **Solution**: Check that you're running the correct binary. The output includes emoji indicators (🚀, ✅, etc.)

## Related Examples

- **[borrowed_migrations](../borrowed_migrations/)** - Working with non-static lifetimes and borrowed data
- **[static_migrations](../static_migrations/)** - Multiple migration discovery methods
- **[basic_migration_test](../basic_migration_test/)** - Testing migrations with `verify_migrations_full_cycle`
