# Basic Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_full_cycle` test utility to ensure your database migrations work correctly in both directions.

## What This Example Demonstrates

- Creating multiple migrations that build upon each other
- Using the schema query builder for database operations
- Testing migrations with `verify_migrations_full_cycle`
- Proper error handling and async migration implementation
- Working with foreign key relationships

## Prerequisites

- Basic understanding of Rust async programming and the `tokio` runtime
- Familiarity with database migrations and why they need testing
- Understanding of foreign key relationships in relational databases

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/basic_migration_test/Cargo.toml
```

This will:

1. Create an in-memory SQLite database
2. Run the migration test using `verify_migrations_full_cycle`
3. Display the results

## Expected Output

```
Basic Migration Test Example
============================

✅ Created in-memory SQLite database
📋 Defined 3 migrations:
  - 001_create_users: Create users table with basic fields
  - 002_add_users_status: Add status column to users table
  - 003_create_posts: Create posts table with foreign key to users

🔄 Testing full migration cycle...
   1. Apply all migrations forward (up)
   2. Verify no errors during forward migration
   3. Apply all migrations backward (down)
   4. Verify database returns to initial state
✅ Full migration cycle completed successfully!

🎉 All migrations work correctly:
   • Forward migrations create tables and indexes properly
   • Backward migrations clean up all changes
   • Database returns to initial empty state

💡 Key Benefits of verify_migrations_full_cycle:
   • Tests both up and down migrations
   • Ensures migrations are reversible
   • Catches migration ordering issues
   • Verifies clean rollback behavior
   • Perfect for CI/CD pipeline testing
```

## Code Walkthrough

The example defines three migrations and tests them with `verify_migrations_full_cycle`:

### 1. Define Migration Structs (src/main.rs:18-199)

Each migration implements the `Migration<'static>` trait:

```rust
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str { "001_create_users" }

    async fn up(&self, db: &dyn Database) -> Result<()> {
        db.create_table("users")
            .column(Column { name: "id".to_string(), ... })
            .primary_key("id")
            .execute(db).await?;

        db.exec_raw("CREATE UNIQUE INDEX idx_users_email ON users(email)").await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> Result<()> {
        db.exec_raw("DROP INDEX IF EXISTS idx_users_email").await?;
        db.exec_raw("DROP TABLE users").await?;
        Ok(())
    }
}
```

### 2. Create Migration List (src/main.rs:222-226)

Collect all migrations in order:

```rust
let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
    Arc::new(CreateUsersTable),
    Arc::new(AddUsersStatusColumn),
    Arc::new(CreatePostsTable),
];
```

### 3. Run Full Cycle Test (src/main.rs:238-258)

Use the test utility to verify migrations work in both directions:

```rust
verify_migrations_full_cycle(db.as_ref(), migrations).await?;
```

This function:

1. Applies all migrations forward (`up()`)
2. Verifies they succeed
3. Rolls them all back (`down()`)
4. Verifies the database returns to initial state

## Key Concepts

### verify_migrations_full_cycle

This test utility is the main focus of the example. It:

- **Tests Reversibility**: Ensures every migration can be rolled back cleanly
- **Catches Errors Early**: Detects migration issues before production
- **Validates Ordering**: Confirms migrations can be applied in sequence
- **Ensures Clean State**: Verifies rollback leaves no artifacts

### Migration Testing Patterns

Three migrations are tested together:

1. **CreateUsersTable** - Base table with unique index
2. **AddUsersStatusColumn** - Schema evolution (adding column)
3. **CreatePostsTable** - Related table with foreign key

This demonstrates testing both simple and complex migration scenarios.

### Raw SQL vs Query Builder

The example shows when to use each approach:

- **Query Builder** - For standard operations (`create_table`, `primary_key`)
- **Raw SQL** - For features not yet in the builder (`UNIQUE INDEX`, `ALTER TABLE ADD`)

## Testing the Example

The example includes unit tests demonstrating different testing patterns:

### Run All Tests

```bash
cargo test --manifest-path packages/switchy/schema/examples/basic_migration_test/Cargo.toml
```

### Individual Test Cases

1. **test_individual_migrations** - Tests a single migration in isolation
2. **test_migration_descriptions** - Validates migration metadata
3. **test_full_cycle_with_test_utils** - Full cycle test in a test context

## Troubleshooting

### Error: "FOREIGN KEY constraint failed"

If foreign key errors occur during rollback:

- Ensure you drop dependent tables before parent tables
- In this example, `posts` (with foreign key to `users`) must be dropped before `users`

### Error: "table already exists"

This indicates the `down()` migration didn't clean up properly:

- Verify your `down()` method drops all created objects
- Check that indexes are dropped before tables
- Use `IF EXISTS` clauses for robustness

### Migration Test Fails on Rollback

If `verify_migrations_full_cycle` fails during the down phase:

- Check that every `up()` operation has a corresponding `down()` operation
- Verify the order of cleanup operations (reverse of creation)
- Use raw `DROP` statements if query builder doesn't support the feature

### SQLite-Specific Issues

Some database features vary by engine:

- SQLite stores datetime as TEXT with special handling
- Use `DataType::Text` with `CURRENT_TIMESTAMP` for timestamps
- Foreign keys must be enabled in SQLite: `PRAGMA foreign_keys = ON`

## Related Examples

- **[basic_usage](../basic_usage/)** - Core migration runner usage without testing utilities
- **[mutation_migration_test](../mutation_migration_test/)** - Testing with data mutations between migrations
- **[state_migration_test](../state_migration_test/)** - Testing data preservation during migrations
