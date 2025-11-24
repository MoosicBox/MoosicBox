# Basic Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_full_cycle` test utility to ensure your database migrations work correctly in both directions.

## What This Example Shows

- Creating multiple migrations that build upon each other
- Using the schema query builder for database operations
- Testing migrations with `verify_migrations_full_cycle`
- Proper error handling and async migration implementation
- Working with foreign key relationships

## Key Features

- **Full Cycle Testing**: Verifies that migrations can be applied (up) and rolled back (down) successfully
- **Schema Query Builder**: Uses `switchy_database`'s modern query builder for table creation, with raw SQL fallback for features not yet supported
- **Type Safety**: Demonstrates proper use of `DataType` enum and `DatabaseValue` for schema definitions

## Prerequisites

- Rust 1.70 or later
- Basic understanding of database migrations and testing
- Familiarity with async Rust

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/basic_migration_test/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/basic_migration_test
cargo run
```

## Expected Output

This will:

1. Create an in-memory SQLite database
2. Run the migration test using `verify_migrations_full_cycle`
3. Display the results

## Migration Structure

The example includes three migrations:

- `CreateUsersTable` (001): Creates a users table with id, name, email, and created_at columns, plus a unique index on email
- `AddUsersStatusColumn` (002): Adds a status column to the users table with a default value
- `CreatePostsTable` (003): Creates a posts table with a foreign key relationship to users

**Note**: Some features (UNIQUE indexes, ALTER TABLE ADD COLUMN) currently use raw SQL as they are not yet supported by the schema query builder.

## Code Walkthrough

### Using verify_migrations_full_cycle

The test utility validates both up and down migrations:

```rust
verify_migrations_full_cycle(
    &db,
    vec![
        Arc::new(CreateUsersTable) as Arc<dyn Migration>,
        Arc::new(AddUsersStatusColumn),
        Arc::new(CreatePostsTable),
    ],
)
.await?;
```

### Creating Migrations with Query Builders

Example of a type-safe table creation:

```rust
db.create_table("users")
    .column(Column {
        name: "id".to_string(),
        data_type: DataType::BigInt,
        nullable: false,
        auto_increment: false,
        default: None,
    })
    .primary_key("id")
    .execute(db)
    .await?;
```

## Key Concepts

1. **Full Cycle Validation**: Tests both forward (up) and backward (down) migration paths
2. **Type-Safe Schemas**: Using `Column` and `DataType` enums instead of raw SQL
3. **Automatic Rollback**: The test utility handles rollback verification automatically
4. **Foreign Key Support**: Demonstrates creating relationships between tables

## Testing the Example

Run the example to verify:

1. All migrations apply successfully (up direction)
2. All migrations roll back successfully (down direction)
3. Database schema is correctly modified at each step
4. Test passes with success message

## Troubleshooting

**Issue**: `error: no bin target named 'basic_migration_test'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

**Issue**: Test fails with "verify_migrations_full_cycle failed"

- **Solution**: Check that each migration's `down()` method properly reverses its `up()` method

## Related Examples

- **[mutation_migration_test](../mutation_migration_test/)** - Testing with data mutations between migrations
- **[state_migration_test](../state_migration_test/)** - Testing with pre-existing data
- **[basic_usage](../basic_usage/)** - Simpler example without test utilities
