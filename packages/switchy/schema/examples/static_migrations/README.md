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

## Running the Example

```bash
cargo run -p static_migrations_example
```

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

## Use Cases

This pattern is ideal for:

- Applications needing full control over migration sources
- Projects using multiple migration discovery methods
- Learning different approaches to defining migrations
- Type-safe migrations using query builders instead of raw SQL
