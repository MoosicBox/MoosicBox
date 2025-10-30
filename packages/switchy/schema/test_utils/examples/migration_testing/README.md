# Migration Testing Example

A comprehensive example demonstrating how to test database migrations using `switchy_schema_test_utils`.

## Summary

This example shows various patterns for testing database schema migrations, including full cycle testing, testing with pre-seeded state, data mutations between migrations, and using the advanced `MigrationTestBuilder` API. These patterns ensure your migrations work correctly and can be safely rolled back.

## What This Example Demonstrates

- **Full cycle testing**: Running migrations forward and then backward to verify reversibility
- **Pre-seeded state testing**: Testing migrations against databases with existing data
- **Data mutation testing**: Inserting data between migration steps to verify behavior
- **MutationBuilder pattern**: Using a fluent API to define data mutations
- **MigrationTestBuilder pattern**: Advanced testing with before/after breakpoints
- **Single migration testing**: Testing migrations in isolation
- **Error handling**: Proper error propagation using `Result` types

## Prerequisites

- Basic understanding of database migrations
- Familiarity with async Rust programming
- Knowledge of SQL and database schema concepts

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/switchy/schema/test_utils/examples/migration_testing/Cargo.toml
```

Or with full warnings:

```bash
cargo run --manifest-path packages/switchy/schema/test_utils/examples/migration_testing/Cargo.toml --features fail-on-warnings
```

## Expected Output

The example runs six different migration testing scenarios and outputs detailed progress for each:

```
╔══════════════════════════════════════════════════════════════╗
║  Migration Testing Example - switchy_schema_test_utils      ║
╚══════════════════════════════════════════════════════════════╝

=== Example 1: Full Cycle Testing ===
Testing migrations forward then backward...

  [UP] Creating users table...
  [UP] Creating posts table...
  [UP] Adding index on posts.user_id...
  [DOWN] Dropping index on posts.user_id...
  [DOWN] Dropping posts table...
  [DOWN] Dropping users table...

✓ Full cycle test passed!

=== Example 2: Testing with Pre-seeded State ===
...

✓ All migration tests passed successfully! ✓
```

## Code Walkthrough

### Example Migrations

The example defines three sample migrations to demonstrate testing patterns:

```rust
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str {
        "001_create_users"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
        ).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP TABLE users").await?;
        Ok(())
    }
}
```

Each migration implements the `Migration` trait with:

- **`id()`**: Unique identifier for the migration
- **`up()`**: Forward migration (schema changes)
- **`down()`**: Rollback migration (undo changes)

### 1. Full Cycle Testing

The simplest testing pattern verifies that migrations can be applied forward and rolled back:

```rust
let db = create_empty_in_memory().await?;
let migrations = create_test_migrations();

verify_migrations_full_cycle(db.as_ref(), migrations).await?;
```

This ensures:

- All migrations apply successfully in sequence
- All migrations can be rolled back in reverse order
- The database returns to its initial state

### 2. Testing with Pre-seeded State

Test migrations against databases that already contain data:

```rust
verify_migrations_with_state(db.as_ref(), migrations, |db| {
    Box::pin(async move {
        db.exec_raw("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)").await?;
        db.exec_raw("INSERT INTO config (key, value) VALUES ('version', '1.0.0')").await?;
        Ok::<(), DatabaseError>(())
    })
}).await?;
```

The setup closure runs before migrations, allowing you to:

- Create existing tables that migrations must work with
- Insert test data that migrations should preserve or transform
- Simulate real-world scenarios where migrations run on populated databases

### 3. Testing with Data Mutations

Insert data between migration steps to verify behavior:

```rust
let mut mutation_map = BTreeMap::new();
mutation_map.insert(
    "001_create_users".to_string(),
    Arc::new("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')".to_string())
        as Arc<dyn Executable>,
);

verify_migrations_with_mutations(db.as_ref(), migrations, mutation_map).await?;
```

This pattern:

- Applies migrations one at a time
- Inserts data after specified migrations
- Verifies subsequent migrations handle the inserted data correctly
- Tests rollback behavior with populated tables

### 4. Using MutationBuilder

A cleaner syntax for defining mutations:

```rust
let mutations = MutationBuilder::new()
    .add_mutation("001_create_users", "INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')")
    .add_mutation("001_create_users", "INSERT INTO users (name, email) VALUES ('Charlie', 'charlie@example.com')")
    .build();

verify_migrations_with_mutations(db.as_ref(), migrations, mutations).await?;
```

Benefits:

- Fluent API for building mutation sets
- Multiple mutations per migration step
- Type-safe construction

### 5. Using MigrationTestBuilder

Advanced testing with before/after breakpoints:

```rust
MigrationTestBuilder::new(migrations)
    .with_table_name("__test_migrations")
    .with_data_after("001_create_users", |db| {
        Box::pin(async move {
            db.exec_raw("INSERT INTO users (name, email) VALUES ('Dave', 'dave@example.com')").await
        })
    })
    .with_data_before("003_add_posts_user_index", |db| {
        Box::pin(async move {
            db.exec_raw("INSERT INTO posts (user_id, title, content) VALUES (1, 'Test Post', 'Content')").await
        })
    })
    .run(db.as_ref())
    .await?;
```

This builder allows:

- **`with_data_after()`**: Execute code after a specific migration
- **`with_data_before()`**: Execute code before a specific migration
- **`with_table_name()`**: Customize the migrations tracking table
- **Multiple breakpoints**: Insert data at any point in the migration sequence

## Key Concepts

### Migration Testing Philosophy

**Why test migrations?**

- Migrations modify production data - errors can be catastrophic
- Rollback paths (`down()`) are often untested until needed
- Data transformations may have edge cases
- Schema changes can break existing data

**What to test:**

- **Forward path**: Migrations apply cleanly in sequence
- **Backward path**: Rollback restores previous state
- **Data preservation**: Existing data survives migrations
- **Data transformation**: Migrations correctly transform data
- **Edge cases**: Migrations handle unusual data patterns

### Testing Patterns

**1. Empty Database Testing** (Example 1, 6)

- Start with a clean database
- Verify schema creation works
- Verify rollback removes all traces
- Use: Initial development, schema structure validation

**2. Pre-seeded State Testing** (Example 2)

- Start with existing tables and data
- Verify migrations work with existing schema
- Verify migrations preserve unrelated data
- Use: Testing migrations on production-like databases

**3. Mutation Testing** (Example 3, 4)

- Insert data between migration steps
- Verify subsequent migrations handle the data
- Verify rollback handles populated tables
- Use: Data migration scenarios, foreign key relationships

**4. Breakpoint Testing** (Example 5)

- Execute code at precise points in migration sequence
- Verify state before/after specific migrations
- Test complex multi-step transformations
- Use: Advanced scenarios, debugging migration issues

### Error Handling

All test functions return `Result<(), TestError>`:

```rust
async fn example() -> Result<(), TestError> {
    let db = create_empty_in_memory().await?; // Can fail
    verify_migrations_full_cycle(db.as_ref(), migrations).await?; // Can fail
    Ok(())
}
```

`TestError` wraps:

- `MigrationError`: Migration execution failures
- `DatabaseError`: Database operation failures
- `DatabaseInit`: Database initialization failures

### In-Memory Testing

The example uses in-memory SQLite databases:

```rust
let db = create_empty_in_memory().await?;
```

Benefits:

- Fast execution (no disk I/O)
- Isolated tests (no shared state)
- No cleanup required
- Perfect for CI/CD pipelines

You can also test against real database files:

```rust
use switchy_database_connection::init_sqlite_sqlx;

let db = init_sqlite_sqlx(Some("test.db")).await?;
```

## Testing the Example

### Run All Tests

```bash
cargo run --manifest-path packages/switchy/schema/test_utils/examples/migration_testing/Cargo.toml
```

### Run with Clippy

```bash
cargo clippy --manifest-path packages/switchy/schema/test_utils/examples/migration_testing/Cargo.toml -- -D warnings
```

### Run with Format Check

```bash
cargo fmt --manifest-path packages/switchy/schema/test_utils/examples/migration_testing/Cargo.toml -- --check
```

### Expected Behavior

- All six examples should pass successfully
- Total execution time: ~1 second
- Output should show detailed migration steps
- Final message confirms all tests passed

## Troubleshooting

### "Database initialization failed"

**Cause**: SQLite driver not available or feature flag missing

**Solution**: Ensure `switchy_schema_test_utils` has the `sqlite` feature enabled in `Cargo.toml`

### "Migration failed: table already exists"

**Cause**: Database not properly cleaned between tests

**Solution**: Use `create_empty_in_memory()` to get a fresh database for each test

### "Foreign key constraint failed"

**Cause**: Test data violates foreign key relationships

**Solution**: Ensure mutations insert data in the correct order (e.g., create users before posts)

### "Migration not found"

**Cause**: Incorrect migration ID in mutation or breakpoint

**Solution**: Verify the migration ID matches the `id()` method exactly (case-sensitive)

## Related Examples

- `packages/switchy/schema/examples/*` - Core migration examples
- `packages/switchy/database/examples/*` - Database connection examples

---

Generated with [Claude Code](https://claude.com/claude-code)
