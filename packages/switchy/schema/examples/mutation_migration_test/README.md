# Mutation Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_mutations` test utility to test migrations with data changes happening between migration steps.

## What This Example Shows

- Creating migrations that handle complex data scenarios
- Using `verify_migrations_with_mutations` for comprehensive testing
- Inserting data between migrations to simulate real-world scenarios
- Testing migrations with foreign key relationships and indexes

## Key Features

- **Mutation Testing**: Tests migrations with data being inserted between migration steps
- **Interleaved Data Changes**: Simulates data modifications during migration sequences
- **Foreign Key Testing**: Validates relationships between users, posts, and analytics tables
- **Index Creation on Populated Tables**: Tests adding indexes after data is already present

## Running the Example

```bash
cargo run --bin mutation_migration_test
```

This will:

1. Create an in-memory SQLite database
2. Run migrations with data mutations applied between specific migrations
3. Verify that migrations handle interleaved data changes correctly
4. Display the results

## Migration Structure

The example includes four migrations:

- `CreateUsersTable`: Creates users table with status and email fields
- `CreatePostsTable`: Creates posts table with user_id foreign key reference
- `CreateAnalyticsTable`: Creates analytics table for tracking user events
- `AddPerformanceIndexes`: Adds indexes on foreign keys and commonly queried fields

## Data Mutations

Data is inserted between migrations using `Executable` implementations:

- After users table: Inserts 3 test users (2 active, 1 inactive)
- After posts table: Inserts 4 test posts linked to users
- After analytics table: Inserts 5 analytics events tracking user activity

## Use Cases

This pattern is ideal for:

- Testing migrations with realistic data patterns
- Ensuring foreign key constraints work correctly
- Validating that indexes can be created on populated tables
- Testing rollback behavior with complex data relationships
- Simulating production scenarios where data exists between migration steps
