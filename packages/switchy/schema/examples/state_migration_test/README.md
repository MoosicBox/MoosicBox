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

## Prerequisites

- Rust 1.70 or later
- Understanding of database migrations and state management
- Familiarity with async Rust and the `Executable` trait

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/state_migration_test/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/state_migration_test
cargo run
```

## Expected Output

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

## Code Walkthrough

### Setting Up Initial State

The example creates initial data before running migrations:

```rust
async fn setup_initial_data(db: &dyn Database) -> Result<(), DatabaseError> {
    db.create_table("users")
        .column(Column { /* ... */ })
        .execute(db)
        .await?;

    db.insert("users")
        .values(vec![
            ("name", DatabaseValue::String("Alice".to_string())),
            ("email", DatabaseValue::String("alice@example.com".to_string())),
        ])
        .execute(db)
        .await?;

    Ok(())
}
```

### Using verify_migrations_with_state

The test utility validates migrations against pre-existing data:

```rust
verify_migrations_with_state(
    &db,
    vec![
        Arc::new(AddUsersBioColumn) as Arc<dyn Migration>,
        Arc::new(AddEmailIndex),
    ],
    Arc::new(setup_initial_data),
)
.await?;
```

### Migration with Default Values

Adding a column with a default value to an existing table:

```rust
async fn up(&self, db: &dyn Database) -> Result<(), MigrationError> {
    db.alter_table("users")
        .add_column("bio", DataType::Text, true, Some(DatabaseValue::String(String::new())))
        .execute(db)
        .await?;
    Ok(())
}
```

## Key Concepts

1. **State Setup**: Initialize database with test data before migrations
2. **Data Preservation**: Ensure existing data survives schema changes
3. **Default Values**: Properly handle new columns with defaults on populated tables
4. **Index Creation**: Create indexes on tables containing data

## Testing the Example

Run the example to verify:

1. Initial state is set up correctly with test data
2. Migrations apply successfully without data loss
3. New columns receive default values for existing rows
4. Indexes are created successfully on populated tables
5. Data integrity is maintained through up/down cycles

## Troubleshooting

**Issue**: `error: no bin target named 'state_migration_test'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

**Issue**: Test fails with "data not preserved"

- **Solution**: Verify that migrations don't truncate or drop tables with existing data

**Issue**: Default value errors on new columns

- **Solution**: Ensure new columns are either nullable or have appropriate default values

## Related Examples

- **[basic_migration_test](../basic_migration_test/)** - Full cycle testing without pre-existing state
- **[mutation_migration_test](../mutation_migration_test/)** - Testing with data mutations between migrations
- **[basic_usage](../basic_usage/)** - Basic migration patterns
