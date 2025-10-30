# Query Builder Example

This example demonstrates the type-safe query builder API in `switchy_database`, showing how to construct SQL queries using a fluent builder pattern instead of writing raw SQL.

## Summary

Learn how to use the query builder to perform database operations (INSERT, SELECT, UPDATE, DELETE, UPSERT) with compile-time type safety and a clean, expressive syntax.

## What This Example Demonstrates

- **INSERT operations**: Adding records using `.insert().value().execute()`
- **SELECT operations**: Querying with `.select().columns().where_*().execute()`
- **UPDATE operations**: Modifying records with `.update().value().where_*().execute()`
- **DELETE operations**: Removing records with `.delete().where_*().execute()`
- **UPSERT operations**: Insert-or-update with `.upsert().value().where_*().execute()`
- **WHERE clauses**: Filtering with `.where_eq()` and `.where_gte()`
- **Column selection**: Specifying which columns to return
- **LIMIT clauses**: Restricting result sets with `.limit()` and `.execute_first()`
- **Value extraction**: Getting typed values from query results

## Prerequisites

- Basic understanding of SQL concepts (INSERT, SELECT, UPDATE, DELETE)
- Familiarity with Rust async/await syntax
- Understanding of the Turso database backend (similar to SQLite)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/database/examples/query_builder/Cargo.toml
```

Or from this directory:

```bash
cargo run
```

## Expected Output

The example will:

1. Create an in-memory Turso database
2. Create `users` and `posts` tables
3. Insert users (alice, bob) and a post
4. Query all users and display them
5. Query specific columns (username and age)
6. Query with WHERE filter (age >= 30)
7. Query with LIMIT (first user only)
8. Update alice's email and status
9. Delete a temporary user
10. Demonstrate UPSERT (update existing, insert new)
11. Show final user count

Example output:

```
=== Switchy Database - Query Builder Example ===

--- INSERT Operations ---
Inserted user 'alice' with ID: 1
Inserted user 'bob' with ID: 2
Inserted post with ID: 1

--- SELECT Operations ---
All users:
  - alice (alice@example.com)
  - bob (bob@example.com)

Usernames only:
  - alice (age: Some(30))
  - bob (age: Some(25))

Users where age >= 30:
  - alice (age: 30)

First user only:
  - alice

--- UPDATE Operations ---
Updated alice's email
Updated alice's age and active status
Alice now: email=alice.new@example.com, age=31, active=false

--- DELETE Operations ---
Created temporary user
Users before delete: 3
Deleted temporary user
Users after delete: 2

--- UPSERT Operations ---
Upserted alice (should update existing)
  Result: email=alice.updated@example.com, age=32
Upserted charlie (should insert new)

Final user count: 3
  - alice: alice.updated@example.com
  - bob: bob@example.com
  - charlie: charlie@example.com

=== Example Complete ===
```

## Code Walkthrough

### Setting Up the Database

```rust
// Create an in-memory Turso database
let db = TursoDatabase::new(":memory:").await?;

// Create tables using raw SQL (for setup only)
db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, ...)").await?;
```

### INSERT - Adding Records

```rust
// Insert using the query builder
let alice = db
    .insert("users")
    .value("username", "alice")
    .value("email", "alice@example.com")
    .value("age", 30)
    .value("active", true)
    .execute(db)
    .await?;

// Get the inserted ID
let alice_id = alice.id().and_then(|v| v.as_i64()).unwrap();
```

### SELECT - Querying Records

```rust
// Select all users (all columns)
let users = db.select("users").execute(db).await?;

// Select specific columns
let usernames = db
    .select("users")
    .columns(&["username", "age"])
    .execute(db)
    .await?;

// Select with WHERE clause (requires FilterableQuery trait import)
use switchy_database::query::FilterableQuery;

let adults = db
    .select("users")
    .where_gte("age", 30)
    .execute(db)
    .await?;

// Select with LIMIT
let first = db
    .select("users")
    .limit(1)
    .execute_first(db)  // Returns Option<Row>
    .await?;
```

### UPDATE - Modifying Records

```rust
// Update a single column
db.update("users")
    .value("email", "alice.new@example.com")
    .where_eq("username", "alice")
    .execute(db)
    .await?;

// Update multiple columns
db.update("users")
    .value("age", 31)
    .value("active", false)
    .where_eq("username", "alice")
    .execute(db)
    .await?;
```

### DELETE - Removing Records

```rust
// Delete records matching a condition
db.delete("users")
    .where_eq("username", "temp_user")
    .execute(db)
    .await?;
```

### UPSERT - Insert or Update

```rust
// Upsert will update if the WHERE condition matches, otherwise insert
db.upsert("users")
    .value("username", "alice")
    .value("email", "alice.updated@example.com")
    .value("age", 32)
    .where_eq("username", "alice")  // Condition for update
    .execute(db)
    .await?;
```

### Extracting Values from Results

```rust
for row in &users {
    // Extract string values (returns borrowed &str, so convert to String)
    let username = row
        .get("username")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap();

    // Extract integer values
    let age = row.get("age").and_then(|v| v.as_i64()).unwrap();

    // Extract boolean values
    let active = row.get("active").and_then(|v| v.as_bool()).unwrap();

    println!("{username}: age={age}, active={active}");
}
```

## Key Concepts

### Query Builder Pattern

The query builder uses method chaining to construct queries in a fluent, readable way:

```rust
db.select("users")      // Start a SELECT query
    .columns(&["id", "name"])  // Specify columns
    .where_eq("active", true)  // Add WHERE clause
    .limit(10)          // Add LIMIT
    .execute(db)        // Execute and return results
    .await?;
```

This approach provides:

- **Type safety**: Compile-time checks for parameter types
- **Readability**: SQL-like structure in Rust
- **Composability**: Build queries programmatically
- **Error prevention**: Harder to make SQL injection mistakes

### FilterableQuery Trait

To use WHERE methods (`where_eq`, `where_gte`, etc.), you must import the trait:

```rust
use switchy_database::query::FilterableQuery;
```

This trait provides:

- `where_eq(left, right)` - Equality condition
- `where_gte(left, right)` - Greater than or equal
- And other comparison operators

### Execute Methods

Different execute methods for different use cases:

- `.execute(db)` - Returns `Vec<Row>` (all matching rows)
- `.execute_first(db)` - Returns `Option<Row>` (first row or None)

For INSERT operations, `.execute()` returns a single `Row` with the inserted data (including auto-generated ID).

### Value Type Conversions

`DatabaseValue` supports various types:

- Strings: `"text".into()` or `String::from("text")`
- Numbers: `42`, `3.14`
- Booleans: `true`, `false`
- Null: `DatabaseValue::Null`

When extracting:

- `.as_str()` - Returns `Option<&str>`
- `.as_i64()` - Returns `Option<i64>`
- `.as_bool()` - Returns `Option<bool>`

## Testing the Example

Run the example and verify the output:

1. Check that INSERT operations create records with IDs
2. Verify SELECT returns the expected users
3. Confirm WHERE filters work correctly (age >= 30 shows only alice)
4. Ensure UPDATE changes are reflected in subsequent queries
5. Verify DELETE removes records (user count decreases)
6. Check UPSERT updates existing records and inserts new ones

## Troubleshooting

### Error: "no method named `where_eq`"

**Solution**: Import the `FilterableQuery` trait:

```rust
use switchy_database::query::FilterableQuery;
```

### Error: "cannot return value referencing function parameter"

**Solution**: When extracting string values, convert to owned String:

```rust
// Wrong:
let name = row.get("name").and_then(|v| v.as_str()).unwrap();

// Correct:
let name = row.get("name").and_then(|v| v.as_str().map(str::to_string)).unwrap();
```

### Database locked errors

This example uses an in-memory database with no concurrent access, so locking shouldn't occur. If using a file-based database with multiple connections, consider using transactions or connection pooling.

## Related Examples

- **[turso_basic](../turso_basic/)**: Basic Turso database operations using raw SQL
- **[turso_transactions](../turso_transactions/)**: Transaction management with commit/rollback

## Notes

- This example uses **Turso** (libSQL), which is API-compatible with SQLite
- The database is **in-memory** (`:memory:`), so data is not persisted
- Query builder methods are **backend-agnostic** - the same code works with SQLite, PostgreSQL, and MySQL
- For production use, prefer the query builder over raw SQL for better type safety and SQL injection protection
- The query builder generates SQL internally - you can use raw SQL with `.exec_raw()` or `.query_raw()` when needed
