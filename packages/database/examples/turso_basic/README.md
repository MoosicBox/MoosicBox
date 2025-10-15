# Turso Database - Basic Usage Example

This example demonstrates the fundamental operations of the Turso Database backend for MoosicBox's `switchy_database` abstraction layer.

## What This Example Demonstrates

- **Database Creation**: Creating an in-memory Turso database
- **Schema Creation**: Creating tables with SQL DDL
- **CRUD Operations**:
    - **Create**: Inserting records with parameterized queries
    - **Read**: Querying data with and without parameters
    - **Update**: Modifying existing records
    - **Delete**: Removing records
- **Schema Introspection**:
    - Checking if tables exist
    - Retrieving table column metadata

## Running the Example

From the repository root:

```bash
cargo run -p turso_basic_example
```

Or from this directory:

```bash
cargo run
```

## Expected Output

The example will:

1. Create an in-memory database
2. Create a `users` table with `id`, `name`, and `email` columns
3. Insert 3 users (Alice, Bob, Charlie)
4. Query all users and display them
5. Query for a specific user by name (Alice)
6. Update a user's email (Charlie)
7. Delete a user (Bob)
8. Show final user count (2 remaining)
9. Demonstrate schema introspection:
    - Check if `users` table exists (true)
    - Check if `posts` table exists (false)
    - List all columns in `users` table with their types

## Code Highlights

### Parameterized Queries

The example uses `.into()` for automatic type conversion:

```rust
db.exec_raw_params(
    "INSERT INTO users (name, email) VALUES (?1, ?2)",
    &[
        "Alice".into(),
        "alice@example.com".into(),
    ],
)
.await?;
```

### Value Extraction

Values are extracted with type safety:

```rust
let id = row.get("id").unwrap().as_i64().unwrap();
let name = row.get("name").unwrap().as_str().unwrap();
```

### Schema Introspection

The database trait provides schema inspection methods:

```rust
let exists = db.table_exists("users").await?;
let columns = db.get_table_columns("users").await?;
```

## Related Examples

- **[turso_transactions](../turso_transactions/)**: Transaction management with commit/rollback

## Notes

- This example uses an **in-memory database** (`:memory:`), so data is not persisted
- All operations use **raw SQL** with parameterized queries
- For file-based persistence, use a file path instead of `:memory:`
