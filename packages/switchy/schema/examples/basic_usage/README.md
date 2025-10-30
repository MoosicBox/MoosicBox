# Basic Usage Example

This example demonstrates the core functionality of `switchy_schema` using type-safe schema builders instead of raw SQL, showing how to create code-based migrations that run against a database.

## What This Example Demonstrates

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

- Basic understanding of Rust async programming and the `tokio` runtime
- Familiarity with database concepts (tables, indexes, columns)
- Understanding of database migrations (what they are and why they're used)

## Running the Example

```bash
cargo run --bin basic_usage
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

## Code Walkthrough

The example follows these key steps:

### 1. Define Migration Structs (src/main.rs:9-133)

Each migration is a struct implementing the `Migration<'static>` trait with `up()` and `down()` methods:

```rust
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str { "001_create_users_table" }

    async fn up(&self, db: &dyn Database) -> Result<(), MigrationError> {
        db.create_table("users")
            .column(Column { name: "id".to_string(), ... })
            .primary_key("id")
            .execute(db)
            .await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> Result<(), MigrationError> {
        db.drop_table("users").if_exists(true).execute(db).await?;
        Ok(())
    }
}
```

### 2. Create Migration Source (src/main.rs:136-149)

Implement `MigrationSource` to provide the list of migrations:

```rust
struct BasicUsageMigrations;

#[async_trait]
impl MigrationSource<'static> for BasicUsageMigrations {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>, MigrationError> {
        Ok(vec![
            Arc::new(CreateUsersTable),
            Arc::new(AddEmailIndex),
            Arc::new(AddCreatedAtColumn),
        ])
    }
}
```

### 3. Setup Database and Runner (src/main.rs:152-168)

Initialize the database connection and create the migration runner:

```rust
let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
let source = BasicUsageMigrations;
let runner = MigrationRunner::new(Box::new(source))
    .with_table_name("__example_migrations".to_string());
```

### 4. Check and Run Migrations (src/main.rs:170-187)

List pending migrations, then run them:

```rust
let migration_info = runner.list_migrations(db).await?;
// Display status...
runner.run(db).await?;
```

### 5. Verify Schema (src/main.rs:189-212)

Insert test data and query to verify the schema works:

```rust
let user_id = db.insert("users")
    .value("name", "Alice Johnson")
    .value("email", "alice@example.com")
    .execute(db).await?;

let users = db.select("users").execute(db).await?;
```

## Key Concepts

### Migration Trait

The `Migration` trait defines the contract for a migration:

- **`id()`**: Unique identifier (conventionally prefixed with a number for ordering)
- **`description()`**: Human-readable description
- **`up()`**: Apply the migration (create table, add column, etc.)
- **`down()`**: Revert the migration (for rollback support)

### Migration Source

The `MigrationSource` trait provides the list of available migrations. The runner uses this to discover what migrations exist and in what order they should run.

### Type-Safe Schema Builders

Instead of raw SQL strings, use the query builder from `switchy_database`:

- `create_table()` - Define tables with `Column` structs
- `create_index()` - Add indexes with type-safe API
- `alter_table()` - Modify existing tables
- `drop_table()`, `drop_index()` - Clean up during rollback

This approach provides compile-time safety and database portability.

### Migration Tracking

The runner automatically creates a migrations table (customizable via `with_table_name()`) to track which migrations have been applied. This prevents re-running migrations and enables status checks.

## Testing the Example

After running the example, you can verify it worked by observing:

1. **Migration status output** - Shows pending migrations before run, applied after
2. **Test user insertion** - Confirms the schema accepts data
3. **Query results** - Displays the inserted user data

To test rollback (optional), uncomment the rollback section at the end of `main()`:

```rust
runner.rollback(db, switchy_schema::RollbackStrategy::Steps(1)).await?;
```

This will revert the last migration (removing the `created_at` column).

## Troubleshooting

### Error: "table already exists"

If you run the example multiple times against a persistent database, you may encounter this error. Solutions:

- Delete the database file and run again
- The example uses in-memory SQLite by default, so restarting should work
- Ensure you're not running multiple instances simultaneously

### Error: "Migration X has not been run yet"

This indicates the migration tracking table is out of sync. This shouldn't happen with a fresh database, but if it does:

- Start with a clean database
- Check that all migrations in the source are in the correct order

### Type errors with `DatabaseValue`

Ensure you're using the correct variant:

- `DatabaseValue::Now` for timestamps with default current time
- Match the `DataType` enum correctly (e.g., `DataType::VarChar(255)`)

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
runner.rollback(db, switchy_schema::RollbackStrategy::Steps(1)).await?;
```

## Related Examples

- **[basic_migration_test](../basic_migration_test/)** - Testing migrations with `verify_migrations_full_cycle`
- **[static_migrations](../static_migrations/)** - Alternative patterns for code-based migrations
- **[mutation_migration_test](../mutation_migration_test/)** - Testing with data between migrations
- **[state_migration_test](../state_migration_test/)** - Testing data preservation during migrations
