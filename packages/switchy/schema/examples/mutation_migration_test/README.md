# Mutation Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_mutations` test utility to test migrations with data changes happening between migration steps, simulating real-world production scenarios.

## What This Example Demonstrates

- Creating migrations that handle complex data scenarios
- Using `verify_migrations_with_mutations` for comprehensive testing
- Inserting data between migrations to simulate real-world scenarios
- Testing migrations with related tables and indexes
- Verifying migrations work correctly with existing data

## Prerequisites

- Basic understanding of Rust async programming and the `tokio` runtime
- Familiarity with database migrations and testing strategies
- Understanding of foreign key relationships and database indexes
- Knowledge of the `Executable` trait from `switchy_database`

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/mutation_migration_test/Cargo.toml
```

This will:

1. Create an in-memory SQLite database
2. Run migrations with data mutations applied between specific migrations
3. Verify that migrations handle interleaved data changes correctly
4. Display the results

## Expected Output

```
Mutation Migration Test Example
===============================

✅ Created in-memory SQLite database
📋 Defined 4 migrations with 3 mutation points
🔄 Testing migrations with data mutations...
   • After migration 001_create_users: Insert test users
   • After migration 002_create_posts: Insert test posts
   • After migration 003_create_analytics: Insert analytics events
✅ All migrations and mutations completed successfully!

🎉 Migrations handled data mutations correctly:
   • Tables accepted data after creation
   • Indexes were created on populated tables
   • Foreign key relationships remained valid
   • No data was lost during schema changes
```

## Code Walkthrough

The example defines four migrations and inserts data between them:

### 1. Define Migrations (src/main.rs:18-200)

Four migrations create a complete schema:

```rust
struct CreateUsersTable;    // 001: users table with status and email
struct CreatePostsTable;     // 002: posts table with user_id FK
struct CreateAnalyticsTable; // 003: analytics table for events
struct AddPerformanceIndexes; // 004: indexes on FK and common columns
```

### 2. Create Data Mutation Functions

Define `Executable` implementations to insert data between migrations:

```rust
struct InsertTestUsers;

#[async_trait]
impl Executable for InsertTestUsers {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.insert("users")
            .value("name", "Alice")
            .value("email", "alice@example.com")
            .execute(db).await?;
        // Insert more test data...
        Ok(())
    }
}
```

### 3. Build Mutation Map (src/main.rs:230-240)

Map migration IDs to data mutations:

```rust
let mut mutations: BTreeMap<String, Vec<Arc<dyn Executable + Send + Sync>>> = BTreeMap::new();
mutations.insert("001_create_users".to_string(), vec![Arc::new(InsertTestUsers)]);
mutations.insert("002_create_posts".to_string(), vec![Arc::new(InsertTestPosts)]);
mutations.insert("003_create_analytics".to_string(), vec![Arc::new(InsertAnalytics)]);
```

### 4. Run Test with Mutations (src/main.rs:245-250)

Use the test utility:

```rust
verify_migrations_with_mutations(db.as_ref(), migrations, mutations).await?;
```

This will:

1. Apply migration 001
2. Run mutations for 001 (insert users)
3. Apply migration 002
4. Run mutations for 002 (insert posts)
5. Continue for all migrations
6. Verify everything succeeded

## Key Concepts

### verify_migrations_with_mutations

This test utility extends `verify_migrations_full_cycle` by allowing data operations between migrations:

- **Real-World Simulation**: Models production where data exists during migrations
- **Relationship Testing**: Verifies foreign keys work with actual data
- **Index Testing**: Ensures indexes can be created on populated tables
- **Data Integrity**: Confirms no data loss during schema changes

### Mutation Points

Mutations run **after** a migration completes, allowing you to:

- Insert test data into newly created tables
- Verify the schema accepts expected data patterns
- Test that subsequent migrations work with existing data
- Simulate production scenarios where tables aren't empty

### The Executable Trait

Data mutations use the `Executable` trait from `switchy_database`:

```rust
#[async_trait]
impl Executable for InsertTestUsers {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        // Your data insertion logic here
    }
}
```

This provides a consistent interface for both migrations and data operations.

## Testing the Example

The example demonstrates a realistic scenario:

1. **Create users table** → Insert test users (2 active, 1 inactive)
2. **Create posts table** → Insert posts linked to users
3. **Create analytics table** → Insert event tracking data
4. **Add performance indexes** → Indexes created on already-populated tables

This tests that:

- Foreign key constraints work correctly
- Indexes can be added to tables with data
- Schema changes don't break existing relationships

## Troubleshooting

### Error: "FOREIGN KEY constraint failed"

If you see this error during data insertion:

- Ensure parent records exist before inserting child records
- Check that foreign key values reference valid IDs
- Verify foreign keys are enabled: `PRAGMA foreign_keys = ON`

### Error: "UNIQUE constraint failed"

This indicates duplicate data in unique columns:

- Check that mutation data doesn't violate unique constraints
- Ensure email addresses, usernames, etc. are unique across mutations
- Use different test data for each mutation point

### Mutations Don't Run

If your mutations aren't executed:

- Verify the mutation map keys match migration IDs exactly
- Check that migrations complete successfully before mutations run
- Ensure `Executable` implementations return `Ok(())`

### Index Creation Fails on Populated Table

If adding indexes to tables with data fails:

- Verify the indexed columns don't contain NULL values (if non-nullable)
- Check for duplicate values if creating a unique index
- Ensure the column exists before creating the index

## Related Examples

- **[basic_migration_test](../basic_migration_test/)** - Testing without data mutations
- **[state_migration_test](../state_migration_test/)** - Testing data preservation during migrations
- **[basic_usage](../basic_usage/)** - Core migration runner without testing utilities
