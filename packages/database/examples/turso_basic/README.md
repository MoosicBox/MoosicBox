# Turso Database - Basic Usage Example

This example demonstrates the fundamental operations of the Turso Database backend for MoosicBox's `switchy_database` abstraction layer.

## Summary

Learn the basics of Turso database operations including CRUD (Create, Read, Update, Delete) using raw SQL with parameterized queries and schema introspection.

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

## Prerequisites

- Basic understanding of SQL syntax
- Familiarity with Rust async/await
- Understanding that Turso is a libSQL database (SQLite-compatible)

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

## Key Concepts

### Parameterized Queries

Using placeholders (`?1`, `?2`) instead of string concatenation prevents SQL injection attacks and provides type safety:

```rust
// Secure - uses parameterized query
db.exec_raw_params("SELECT * FROM users WHERE name = ?1", &["Alice".into()]).await?;

// Insecure - DO NOT DO THIS (vulnerable to SQL injection)
// let name = user_input;
// db.exec_raw(&format!("SELECT * FROM users WHERE name = '{}'", name)).await?;
```

### Schema Introspection

The `switchy_database` abstraction provides methods to inspect database structure at runtime:

- `table_exists(name)` - Check if a table exists
- `get_table_columns(name)` - Get column metadata (name, type, nullable, etc.)
- `list_tables()` - Get all table names

This is useful for migrations, validation, and dynamic schema handling.

### Turso vs SQLite

Turso is built on libSQL (a fork of SQLite) and is API-compatible with SQLite. The main differences:

- Turso supports cloud-hosted databases
- Turso provides edge replication
- For local development, behavior is identical to SQLite

## Testing the Example

Run the example and verify:

1. The `users` table is created successfully
2. Three users are inserted (Alice, Bob, Charlie)
3. Querying returns all inserted users
4. Parameterized query finds Alice
5. Charlie's email is updated
6. Bob is deleted
7. Final count shows 2 users remaining
8. Schema introspection correctly identifies the `users` table exists

## Troubleshooting

### Error: "no such table: users"

The table creation might have failed. Check that the `exec_raw` call for CREATE TABLE succeeded without errors.

### Error: "database is locked"

Turso in-memory databases shouldn't lock. If using a file-based database, ensure no other process has it open.

### Incorrect query results

Verify parameterized queries use the correct placeholder syntax (`?1`, `?2`, etc.) and parameters are passed in the correct order.

## Related Examples

- **[turso_transactions](../turso_transactions/)**: Transaction management with commit/rollback
- **[query_builder](../query_builder/)**: Type-safe query builder API

## Notes

- This example uses an **in-memory database** (`:memory:`), so data is not persisted
- All operations use **raw SQL** with parameterized queries
- For file-based persistence, use a file path instead of `:memory:`
