# State Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_state` test utility to test migrations that need to preserve and validate data during schema changes.

## What This Example Shows

- Creating migrations that modify existing data
- Using `verify_migrations_with_state` to test data preservation
- Implementing state setup and validation functions
- Working with existing data during schema migrations

## Key Features

- **State Preservation**: Tests that data survives migration up/down cycles
- **Data Validation**: Verifies that data transformations work correctly
- **Real-world Scenarios**: Demonstrates adding columns with default values and updating existing records
- **Schema Query Builder**: Uses modern query builder syntax for all database operations

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
- `CreateUsersTable`: Initial table creation
- `AddUserStatus`: Migration that adds a status column with default value
- State setup function that inserts test users
- State validation function that checks data integrity after migrations

## Use Cases

This pattern is ideal for:
- Adding columns with default values
- Migrating data between schema versions
- Ensuring no data loss during migrations
- Testing complex data transformations