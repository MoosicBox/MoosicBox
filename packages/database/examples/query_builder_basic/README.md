# Query Builder API - Basic Usage Example

This example demonstrates the core query builder API of switchy_database, which provides database-agnostic operations for SELECT, INSERT, UPDATE, DELETE, and UPSERT across SQLite, PostgreSQL, and MySQL backends.

## Summary

This example shows how to use switchy_database's query builder API instead of writing raw SQL. The query builder provides type-safe, database-agnostic operations that work identically across all supported backends (SQLite, PostgreSQL, MySQL, Turso).

## What This Example Demonstrates

- **Schema Creation API**: Creating tables using portable schema definitions instead of raw DDL
- **INSERT Operations**: Using `.insert()` with `.value()` chains and automatic ID retrieval
- **SELECT Operations**: Using `.select()` with column selection, WHERE clauses, and ORDER BY
- **UPDATE Operations**: Using `.update()` to modify records with type-safe value setting
- **UPSERT Operations**: Using `.upsert()` for insert-or-update semantics
- **DELETE Operations**: Using `.delete()` with WHERE conditions
- **Transactions**: Using `.begin_transaction()` with commit and rollback
- **Query Execution Pattern**: The `.execute(&*db)` pattern for running queries

## Prerequisites

- Basic understanding of database concepts (tables, queries, transactions)
- Familiarity with Rust async/await syntax
- No external database setup required (example uses in-memory SQLite)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/database/examples/query_builder_basic/Cargo.toml
```

Or using the package alias:

```bash
cargo run -p switchy_database_query_builder_basic_example
```

## Expected Output

The example will:

1. Create an in-memory SQLite database
2. Create a `products` table with id, name, price, and stock columns
3. Insert three products (Laptop, Mouse, Keyboard) and display their auto-generated IDs
4. Query all products and display them
5. Query products with price > $50 (Laptop and Keyboard)
6. Query products ordered by price descending
7. Update Laptop stock from 10 to 15
8. Perform upsert to insert Monitor, then update it
9. Delete Mouse product
10. Demonstrate transaction rollback (set all prices to 0, then rollback)
11. Demonstrate transaction commit (apply 10% discount, then commit)
12. Display final product listing with discounted prices

Example output:

```
Query Builder API - Basic Usage Example
========================================

Creating in-memory SQLite database...
✓ Database created

Creating 'products' table using schema API...
✓ Table created

Inserting products using query builder...
  * Inserted Laptop with ID: 1
  * Inserted Mouse with ID: 2
  * Inserted Keyboard with ID: 3

Querying products...
  Found 3 products:
    - ID 1: Laptop - $999.99 (Stock: 10)
    - ID 2: Mouse - $25.50 (Stock: 50)
    - ID 3: Keyboard - $75.00 (Stock: 30)

[...continues with all operations...]

Final product listing:
  * Laptop: $899.99 (Stock: 15)
  * Keyboard: $67.50 (Stock: 30)
  * Monitor: $292.50 (Stock: 25)

✓ Example completed successfully!
```

## Code Walkthrough

### Database Initialization

```rust
let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
```

Uses the connection helper to create an in-memory SQLite database. Pass `Some(&path)` for file-based storage.

### Schema Creation

```rust
create_table("products")
    .column(Column {
        name: "id".to_string(),
        nullable: false,
        auto_increment: true,
        data_type: DataType::BigInt,
        default: None,
    })
    .column(Column {
        name: "name".to_string(),
        nullable: false,
        data_type: DataType::VarChar(100),
        default: None,
    })
    .primary_key("id")
    .execute(&*db)
    .await?;
```

The schema API generates backend-specific SQL automatically. `DataType::BigInt` becomes `INTEGER` on SQLite, `BIGINT` on PostgreSQL/MySQL.

### INSERT with Value Retrieval

```rust
let laptop = db
    .insert("products")
    .value("name", "Laptop")
    .value("price", 999.99)
    .value("stock", 10)
    .execute(&*db)
    .await?;

let laptop_id = laptop.id().and_then(|v| v.as_i64()).unwrap();
```

INSERT operations return the inserted row, including auto-generated IDs. The `.value()` method accepts any type implementing `Into<DatabaseValue>`.

### SELECT with Filtering

```rust
let expensive_products = db
    .select("products")
    .columns(&["name", "price"])
    .where_gt("price", 50.0)
    .execute(&*db)
    .await?;
```

The query builder provides methods like `.where_eq()`, `.where_gt()`, `.where_lt()`, etc. for type-safe filtering.

### UPDATE Operations

```rust
let updated_rows = db
    .update("products")
    .value("stock", 15)
    .where_eq("id", laptop_id)
    .execute(&*db)
    .await?;
```

UPDATE returns all modified rows. Use `.execute_first()` to get just the first row.

### UPSERT Operations

```rust
let monitor = db
    .upsert("products")
    .value("name", "Monitor")
    .value("price", 350.00)
    .unique(&["name"])
    .execute(&*db)
    .await?;
```

UPSERT inserts a new row or updates an existing one based on the unique column(s). Different backends use different mechanisms (UPSERT on SQLite, ON CONFLICT on PostgreSQL, etc.), but the API is identical.

### Transactions

```rust
let tx = db.begin_transaction().await?;

// Execute operations using &*tx instead of &*db
tx.update("products")
    .value("price", 0.0)
    .execute(&*tx)
    .await?;

// Either commit or rollback
tx.rollback().await?;
// or
tx.commit().await?;
```

The critical pattern is `.execute(&*tx)` - operations must be executed on the transaction object, not the original database.

### Value Extraction

```rust
let name = product.get("name").and_then(|v| v.as_str()).unwrap();
let price = product.get("price").and_then(|v| v.as_f64()).unwrap();
let stock = product.get("stock").and_then(|v| v.as_i32()).unwrap();
```

Use `.as_str()`, `.as_i64()`, `.as_f64()`, etc. to extract typed values from `DatabaseValue`.

## Key Concepts

### Database Abstraction

The query builder API abstracts over different database backends. The same code works with:

- SQLite (rusqlite or sqlx)
- PostgreSQL (raw tokio-postgres or sqlx)
- MySQL (sqlx)
- Turso

Switch backends by changing the initialization function and feature flags.

### The Execute Pattern

All query builders require `.execute(&*db)` or `.execute(&*tx)` to run:

```rust
db.select("table").execute(&*db).await?  // Execute on database
tx.insert("table").execute(&*tx).await?  // Execute on transaction
```

This pattern ensures type safety and prevents accidental execution on the wrong database connection.

### Type Safety

The query builder accepts Rust types directly:

```rust
.value("price", 999.99)      // f64 -> DatabaseValue::Real64
.value("stock", 10)          // i32 -> DatabaseValue::Int32
.value("name", "Laptop")     // &str -> DatabaseValue::String
```

Conversions happen automatically via `Into<DatabaseValue>`.

### Transaction Isolation

Transactions operate on dedicated connections from the connection pool, ensuring:

- Operations within a transaction see consistent data
- Other connections don't see uncommitted changes
- Rollback reverts all operations since `begin_transaction()`
- Commit makes all operations permanent

## Testing the Example

The example is self-contained and demonstrates all operations with expected results. You can modify it to:

1. **Change the backend**: Replace `init_sqlite_sqlx()` with `init_postgres_sqlx()` or similar
2. **Use a file-based database**: Pass `Some(&path)` instead of `None` to persist data
3. **Add more complex queries**: Try `.where_in()`, `.limit()`, `.offset()`, etc.
4. **Experiment with savepoints**: Use `tx.savepoint("name")` for nested transaction-like behavior

## Troubleshooting

### "No such table" errors

The example creates tables programmatically. If using a persistent database file, ensure the schema is created or the file is deleted between runs.

### Type conversion errors

Ensure you're using the correct `.as_*()` method for the column's data type:

- Use `.as_i64()` for BigInt columns
- Use `.as_f64()` for Real/Double columns
- Use `.as_str()` for Text/VarChar columns

### Transaction already committed/rolled back

Transactions can only be committed or rolled back once. The ownership system prevents reuse after commit/rollback.

## Related Examples

- **[turso_basic](../turso_basic/)**: Raw SQL operations with Turso backend
- **[turso_transactions](../turso_transactions/)**: Transaction examples using raw SQL

## Notes

- This example uses **SQLite** for simplicity, but the same code works with all backends
- The query builder API is the **recommended way** to use switchy_database (vs. raw SQL)
- For schema introspection capabilities, see the main package README
- For savepoint examples (nested transactions), see the integration tests
