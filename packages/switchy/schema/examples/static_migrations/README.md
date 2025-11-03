# Static Migrations Example

This example demonstrates how to use `switchy_schema` with static lifetime migrations, covering multiple migration discovery methods and patterns.

## What This Example Shows

- Creating custom migrations with `'static` lifetime
- Using code-based migrations with raw SQL
- Using code-based migrations with query builders
- Building custom migration sources
- Three discovery methods: custom, directory, and code-based

## Key Features

- **Static Lifetimes**: Migrations defined with `'static` lifetime that own their data
- **Custom Migration Sources**: Build your own migration source implementations
- **Code Migrations**: Define migrations programmatically with `CodeMigrationSource`
- **Query Builders**: Create migrations using type-safe query builders from `switchy_database`
- **Multiple Discovery Methods**: Examples of custom, directory, and code-based approaches

## Prerequisites

- Rust 1.70 or later
- Basic understanding of database schemas
- Familiarity with async Rust and trait implementations

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/static_migrations/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/static_migrations
cargo run
```

## Expected Output

This will:

1. Demonstrate a custom migration source with owned migrations
2. Show directory-based migration loading (example code only)
3. Create code-based migrations using raw SQL strings
4. Create code-based migrations using query builders
5. Display all migration patterns without applying to a database

## Migration Patterns

The example demonstrates:

### 1. Custom Migration Source

- Custom `Migration<'static>` implementation
- Custom `MigrationSource<'static>` implementation
- Owned data structures with `String` types
- Example migrations: users table, posts table, and indexes

### 2. Directory-based (Code Example)

- Commented example showing directory-based discovery
- Would load from `./migrations` directory
- Each subdirectory becomes a migration with `up.sql` and `down.sql`

### 3. Code Migration Source (Raw SQL)

- Using `CodeMigrationSource::new()`
- Adding migrations with raw SQL strings
- Example migrations: categories and tags tables

### 4. Code Migration Source (Query Builders)

- Using `create_table()` query builder from `switchy_database`
- Type-safe schema definitions with `Column` and `DataType`
- Example migration: products table with multiple columns and primary key

## Dependencies

Based on `Cargo.toml`:

- `switchy_schema` with features: `code`, `directory`, `embedded`
- `switchy_database` for query builders
- `switchy_async` with `macros` and `tokio` features
- `async-trait` for async trait implementations
- `tokio` with full features

## Use Cases

This pattern is ideal for:

- Applications needing full control over migration sources
- Projects using multiple migration discovery methods
- Learning different approaches to defining migrations
- Type-safe migrations using query builders instead of raw SQL

## Code Walkthrough

### 1. Custom Migration Implementation

The example shows how to implement the `Migration` trait for owned data:

```rust
struct OwnedMigration {
    id: String,
    description: String,
    up_sql: String,
    down_sql: Option<String>,
}

#[async_trait]
impl Migration<'static> for OwnedMigration {
    fn id(&self) -> &str {
        &self.id
    }
    // ...
}
```

### 2. Code Migration Source (Raw SQL)

Using `CodeMigrationSource` for raw SQL:

```rust
let mut source = CodeMigrationSource::new();
source.add_migration(CodeMigration::new(
    "001_create_categories".to_string(),
    Box::new("CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT)".to_string()),
    Some(Box::new("DROP TABLE categories".to_string())),
));
```

### 3. Query Builder Migrations

Type-safe schema definition with query builders:

```rust
db.create_table("products")
    .column(Column {
        name: "id".to_string(),
        data_type: DataType::BigInt,
        nullable: false,
        auto_increment: true,
        default: None,
    })
    .primary_key("id")
    .execute(db)
    .await?
```

## Key Concepts

1. **Owned Data**: Using `String` instead of `&str` for `'static` lifetime
2. **Custom Sources**: Implementing `MigrationSource` trait for full control
3. **Code-Based Migrations**: Programmatically defining migrations without files
4. **Type Safety**: Using query builders for compile-time SQL validation

## Testing the Example

The example demonstrates patterns without applying to a database. To use these patterns:

1. Choose the migration source that fits your use case
2. Integrate with `MigrationRunner::new(Box::new(your_source))`
3. Run against your database with `runner.run(db).await?`

## Troubleshooting

**Issue**: `error: no bin target named 'static_migrations_example'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

**Issue**: Directory-based example code doesn't compile when uncommented

- **Solution**: Ensure the `./migrations` directory exists and contains properly formatted migration subdirectories

## Related Examples

- **[basic_usage](../basic_usage/)** - Complete runnable example with code-based migrations
- **[borrowed_migrations](../borrowed_migrations/)** - Working with non-static lifetimes
