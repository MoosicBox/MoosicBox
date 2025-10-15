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

## Running the Example

```bash
cargo run --bin basic_migration_test
```

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

This is the simplest way to get started with `switchy_schema` migration testing.
