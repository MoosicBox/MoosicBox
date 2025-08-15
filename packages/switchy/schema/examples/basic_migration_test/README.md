# Basic Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_full_cycle` test utility to ensure your database migrations work correctly in both directions.

## What This Example Shows

- Creating a simple migration that adds a `users` table
- Using the schema query builder for database operations
- Testing migrations with `verify_migrations_full_cycle`
- Proper error handling and async migration implementation

## Key Features

- **Full Cycle Testing**: Verifies that migrations can be applied (up) and rolled back (down) successfully
- **Schema Query Builder**: Uses `switchy_database`'s modern query builder instead of raw SQL
- **Type Safety**: Demonstrates proper use of `DataType` enum and `DatabaseValue` for schema definitions

## Running the Example

```bash
cargo run --bin basic_migration_test
```

This will:
1. Create an in-memory SQLite database
2. Run the migration test using `verify_migrations_full_cycle`
3. Display the results

## Migration Structure

The example includes:
- `CreateUsersTable`: A migration that creates a users table with id, name, email, and created_at columns
- Proper up/down implementation using the schema query builder
- Error handling with custom error types

This is the simplest way to get started with `switchy_schema` migration testing.