# Mutation Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_mutations` test utility to test migrations with data changes happening between migration steps.

## What This Example Shows

- Creating migrations that handle complex data scenarios
- Using `verify_migrations_with_mutations` for comprehensive testing
- Inserting data between migrations to simulate real-world scenarios
- Testing migrations with related tables and indexes

## Key Features

- **Mutation Testing**: Tests migrations with data being inserted between migration steps
- **Interleaved Data Changes**: Simulates data modifications during migration sequences
- **Related Table Testing**: Tests migrations with related tables (users, posts, and analytics)
- **Index Creation on Populated Tables**: Tests adding indexes after data is already present

## Prerequisites

- Rust 1.70 or later
- Understanding of database migrations and testing
- Familiarity with async Rust and the `Executable` trait

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/mutation_migration_test/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/mutation_migration_test
cargo run
```

## Expected Output

This will:

1. Create an in-memory SQLite database
2. Run migrations with data mutations applied between specific migrations
3. Verify that migrations handle interleaved data changes correctly
4. Display the results

## Migration Structure

The example includes four migrations:

- `CreateUsersTable`: Creates users table with status and email fields
- `CreatePostsTable`: Creates posts table with user_id relationship to users
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
- Ensuring relationships between tables work correctly
- Validating that indexes can be created on populated tables
- Testing rollback behavior with complex data relationships
- Simulating production scenarios where data exists between migration steps

## Code Walkthrough

### Using verify_migrations_with_mutations

The test utility accepts migrations paired with data mutations:

```rust
verify_migrations_with_mutations(
    &db,
    vec![
        (Arc::new(CreateUsersTable) as Arc<dyn Migration>, Some(insert_users)),
        (Arc::new(CreatePostsTable), Some(insert_posts)),
        (Arc::new(CreateAnalyticsTable), Some(insert_analytics)),
        (Arc::new(AddPerformanceIndexes), None),
    ],
)
.await?;
```

### Defining Data Mutations

Mutations implement `Executable` to insert test data:

```rust
struct InsertUsers;

#[async_trait]
impl Executable for InsertUsers {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.insert("users")
            .values(vec![
                ("name", DatabaseValue::String("Alice".to_string())),
                ("email", DatabaseValue::String("alice@example.com".to_string())),
            ])
            .execute(db)
            .await?;
        Ok(())
    }
}
```

## Key Concepts

1. **Interleaved Mutations**: Data is inserted between migration steps to test realistic scenarios
2. **Foreign Key Testing**: Ensures relationships work correctly when data exists
3. **Index Performance**: Tests that indexes can be created on populated tables
4. **Rollback with Data**: Verifies migrations roll back correctly even with data present

## Testing the Example

Run the example and verify:

1. Migrations apply successfully with data inserted between steps
2. Foreign key relationships are maintained correctly
3. Indexes are created successfully on populated tables
4. Rollback works correctly with existing data

## Troubleshooting

**Issue**: `error: no bin target named 'mutation_migration_test'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

**Issue**: Foreign key constraint violations during test

- **Solution**: Ensure data mutations insert records in the correct order (parent tables before child tables)

**Issue**: Test fails during rollback

- **Solution**: Verify that `down()` methods properly handle tables with existing data

## Related Examples

- **[basic_migration_test](../basic_migration_test/)** - Simpler full cycle testing without data mutations
- **[state_migration_test](../state_migration_test/)** - Testing with pre-existing state
- **[basic_usage](../basic_usage/)** - Basic migration patterns
