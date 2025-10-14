# State Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_state` test utility to test migrations that need to preserve and validate data during schema changes.

## What This Example Shows

- Creating migrations that work with existing data
- Using `verify_migrations_with_state` to test data preservation
- Implementing state setup functions for pre-existing data
- Working with existing data during schema migrations

## Key Features

- **State Preservation**: Tests that data survives migration up/down cycles
- **Data Integrity**: Verifies that migrations handle pre-existing data correctly
- **Real-world Scenarios**: Demonstrates adding columns with default values and creating indexes on existing tables
- **Schema Query Builder**: Uses modern query builder syntax for table creation and data insertion

## Running the Example

```bash
cargo run --bin state_migration_test
```

This will:
1. Create an in-memory SQLite database
2. Set up initial state with test data
3. Run migrations and verify data integrity
4. Display the results

## Migration Structure

The example includes:
- `setup_initial_data`: Function that creates the initial users table with 3 test users
- `AddUsersBioColumn`: Migration that adds a `bio` column with a default empty string value
- `AddEmailIndex`: Migration that creates an index on the `email` column
- `verify_migrations_with_state`: Test utility that validates migrations against pre-existing data

## Use Cases

This pattern is ideal for:
- Adding columns with default values to existing tables
- Creating indexes on tables with existing data
- Ensuring no data loss during migrations
- Testing migrations in production-like scenarios with pre-existing data