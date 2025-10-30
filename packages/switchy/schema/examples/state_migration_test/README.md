# State Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_state` test utility to test migrations that need to preserve and validate data during schema changes.

## What This Example Demonstrates

- Creating migrations that work with existing data
- Using `verify_migrations_with_state` to test data preservation
- Implementing state setup functions for pre-existing data
- Working with existing data during schema migrations
- Adding columns with default values to populated tables

## Prerequisites

- Basic understanding of Rust async programming and the `tokio` runtime
- Familiarity with database migrations and data preservation requirements
- Understanding of schema evolution patterns (adding columns, indexes)
- Knowledge of the test utilities in `switchy_schema_test_utils`

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/state_migration_test/Cargo.toml
```

This will:

1. Create an in-memory SQLite database
2. Set up initial state with test data
3. Run migrations and verify data integrity
4. Display the results

## Expected Output

```
State Migration Test Example
============================

✅ Created in-memory SQLite database
📋 Setting up initial database state...
   • Created users table
   • Inserted 3 test users
🔄 Testing migrations with pre-existing state...
   • Migration 001_add_bio_column
   • Migration 002_add_email_index
✅ All migrations completed with data preserved!

🎉 State preserved successfully:
   • All original users remain in database
   • New bio column added with default values
   • Email index created on existing data
   • No data loss during schema changes
```

## Code Walkthrough

The example sets up initial state, then runs migrations that modify the schema:

### 1. Define State Setup Function (src/main.rs:15-50)

Create initial database state with test data:

```rust
async fn setup_initial_data(db: &dyn Database) -> Result<()> {
    // Create initial users table
    db.create_table("users")
        .column(Column { name: "id".to_string(), ... })
        .primary_key("id")
        .execute(db).await?;

    // Insert test data
    db.insert("users")
        .value("name", "Alice")
        .value("email", "alice@example.com")
        .execute(db).await?;

    Ok(())
}
```

### 2. Define Migrations (src/main.rs:52-120)

Migrations that work with existing data:

```rust
struct AddUsersBioColumn;

#[async_trait]
impl Migration<'static> for AddUsersBioColumn {
    fn id(&self) -> &str { "001_add_bio_column" }

    async fn up(&self, db: &dyn Database) -> Result<()> {
        db.alter_table("users")
            .add_column("bio".to_string(), DataType::Text, true, Some(DatabaseValue::String("".to_string())))
            .execute(db).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> Result<()> {
        db.alter_table("users")
            .drop_column("bio".to_string())
            .execute(db).await?;
        Ok(())
    }
}
```

### 3. Run Test with State (src/main.rs:125-135)

Use the test utility to verify data preservation:

```rust
let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
    Arc::new(AddUsersBioColumn),
    Arc::new(AddEmailIndex),
];

verify_migrations_with_state(
    db.as_ref(),
    migrations,
    setup_initial_data
).await?;
```

This will:

1. Call `setup_initial_data` to create initial state
2. Apply each migration
3. Verify data integrity after each migration
4. Confirm original data is preserved

## Key Concepts

### verify_migrations_with_state

This test utility ensures migrations preserve existing data:

- **Data Preservation**: Verifies no data loss during schema changes
- **State Testing**: Models real production scenarios with existing data
- **Schema Evolution**: Tests adding columns, indexes to populated tables
- **Integrity Validation**: Confirms relationships and constraints remain valid

### State Setup Function

The state setup function creates the initial database state:

```rust
async fn setup_initial_data(db: &dyn Database) -> Result<()> {
    // Create tables and insert test data
}
```

This simulates a production database before migrations run.

### Adding Columns with Defaults

When adding columns to populated tables:

```rust
db.alter_table("users")
    .add_column("bio".to_string(), DataType::Text, true, Some(DatabaseValue::String("".to_string())))
    .execute(db).await?;
```

- Set `nullable: true` OR provide a default value
- Default values are applied to all existing rows
- Verify the default makes sense for your data

### Creating Indexes on Existing Data

Indexes can be added after data exists:

```rust
db.create_index("idx_users_email")
    .table("users")
    .column("email")
    .if_not_exists(true)
    .execute(db).await?;
```

- Ensure indexed columns don't contain invalid data
- Use `UNIQUE` indexes carefully with existing data
- Index creation may be slow on large tables

## Testing the Example

The example demonstrates common schema evolution patterns:

1. **Initial State**: Users table with 3 test users
2. **Add bio column**: New optional column with empty string default
3. **Add email index**: Index created on existing email column

This tests realistic scenarios:

- Adding new optional fields to existing entities
- Improving query performance with indexes
- Ensuring no data loss during migrations

### Verify Data Preservation

After running, verify:

- All original users still exist
- New `bio` column has default value for existing rows
- Email index exists and is usable for queries

## Troubleshooting

### Error: "Cannot add NOT NULL column without default"

If you try to add a non-nullable column without a default:

- Make the column nullable: `nullable: true`
- Provide a default value: `Some(DatabaseValue::String("default".to_string()))`
- Or use a two-step migration: add nullable, populate, make non-nullable

### Error: "UNIQUE constraint violation"

When creating a unique index on existing data:

- Check for duplicate values in the indexed column
- Clean up duplicates before creating the index
- Or make the index non-unique

### State Setup Fails

If `setup_initial_data` fails:

- Ensure the database is empty before setup runs
- Check that table creation succeeds
- Verify test data doesn't violate constraints

### Migration Doesn't Preserve Data

If data is lost during migration:

- Check that your `down()` method doesn't delete data unnecessarily
- Verify `ALTER TABLE` operations are correct
- Use transactions to ensure atomicity

## Related Examples

- **[basic_migration_test](../basic_migration_test/)** - Testing without pre-existing data
- **[mutation_migration_test](../mutation_migration_test/)** - Testing with data between migrations
- **[basic_usage](../basic_usage/)** - Core migration runner without testing utilities
