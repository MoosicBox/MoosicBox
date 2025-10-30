# Static Migrations Example

This example demonstrates how to use `switchy_schema` with static lifetime migrations, covering multiple migration discovery methods and patterns including custom sources, directory-based loading, and code-based definitions.

## What This Example Demonstrates

- Creating custom migrations with `'static` lifetime
- Using code-based migrations with raw SQL
- Using code-based migrations with query builders
- Building custom migration sources
- Three discovery methods: custom, directory, and code-based

## Prerequisites

- Basic understanding of Rust ownership and `'static` lifetimes
- Familiarity with `switchy_schema` core concepts
- Understanding of the difference between borrowed and owned data
- Knowledge of migration discovery patterns

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/static_migrations/Cargo.toml
```

This will:

1. Demonstrate a custom migration source with owned migrations
2. Show directory-based migration loading (example code only)
3. Create code-based migrations using raw SQL strings
4. Create code-based migrations using query builders
5. Display all migration patterns without applying to a database

## Expected Output

```
Static Migrations Example
=========================

📋 Pattern 1: Custom Migration Source
   • 001_create_users: Create users table
   • 002_create_posts: Create posts table
   • 003_add_user_index: Add index on users

📋 Pattern 2: Directory-Based Discovery (code example)
   • Loads from ./migrations directory
   • Each subdirectory = one migration
   • up.sql and down.sql in each directory

📋 Pattern 3: Code Migration Source (Raw SQL)
   • 001_create_categories: CREATE TABLE categories...
   • 002_create_tags: CREATE TABLE tags...

📋 Pattern 4: Code Migration Source (Query Builders)
   • 001_create_products: Type-safe table creation

🎉 All patterns demonstrated successfully!
```

## Code Walkthrough

### 1. Custom Migration Source with Static Lifetimes

Define migrations with owned data:

```rust
struct CreateUsersTable {
    id: String,
    description: String,
}

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str { &self.id }

    async fn up(&self, db: &dyn Database) -> Result<()> {
        db.create_table("users")
            .column(Column { ... })
            .execute(db).await?;
        Ok(())
    }
}
```

### 2. Custom Migration Source Implementation

Implement `MigrationSource` for your custom source:

```rust
struct CustomMigrationSource {
    migrations: Vec<Arc<dyn Migration<'static> + 'static>>,
}

#[async_trait]
impl MigrationSource<'static> for CustomMigrationSource {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        Ok(self.migrations.clone())
    }
}
```

### 3. Directory-Based Discovery (Code Example)

```rust
#[cfg(feature = "directory")]
{
    use switchy_schema::runner::MigrationRunner;

    let runner = MigrationRunner::new_directory("./migrations");
    // Each subdirectory under ./migrations becomes a migration
    // with up.sql and down.sql files
}
```

### 4. Code Migration Source with Raw SQL

```rust
use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};

let mut source = CodeMigrationSource::new();
source.add_migration(CodeMigration::new(
    "001_create_categories".to_string(),
    Box::new("CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT)".to_string()),
    Some(Box::new("DROP TABLE categories".to_string())),
));

let runner = MigrationRunner::new(Box::new(source));
```

### 5. Code Migration Source with Query Builders

```rust
let mut source = CodeMigrationSource::new();
source.add_migration(CodeMigration::new_with_builder(
    "001_create_products".to_string(),
    Box::new(|db: &dyn Database| {
        Box::pin(async move {
            db.create_table("products")
                .column(Column { name: "id".to_string(), ... })
                .primary_key("id")
                .execute(db).await
        })
    }),
    None,
));
```

## Key Concepts

### Static Lifetimes

Migrations with `'static` lifetime own their data:

```rust
impl Migration<'static> for MyMigration {
    // All data in this migration is owned (String, not &str)
}
```

This is the most common pattern and simplifies lifetime management.

### Migration Discovery Methods

Three approaches to discovering migrations:

1. **Custom Source**: Full control, define migrations in Rust code
2. **Directory**: Load from filesystem at runtime (`./migrations/001_name/{up,down}.sql`)
3. **Code-Based**: Define programmatically using `CodeMigrationSource`

### When to Use Each Method

- **Custom Source**: Maximum flexibility, complex logic, compile-time safety
- **Directory**: Easy to maintain, non-Rust developers can edit, runtime flexibility
- **Code-Based**: Dynamic generation, templating, programmatic construction
- **Embedded**: Similar to directory but compiled into binary (see embedded example)

### Raw SQL vs Query Builder

**Raw SQL**:

```rust
Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string())
```

- Direct control over SQL
- Database-specific features
- Less type safety

**Query Builder**:

```rust
db.create_table("users")
    .column(Column { name: "id".to_string(), data_type: DataType::Int, ... })
    .primary_key("id")
```

- Type-safe
- Database-agnostic
- Compile-time verification

## Testing the Example

This example is primarily educational, demonstrating different patterns. To use in your project:

1. Choose a discovery method based on your needs
2. Implement migrations using chosen pattern
3. Create `MigrationRunner` with appropriate source
4. Call `runner.run(db).await?`

## Troubleshooting

### Error: "cannot move out of borrowed content"

If you get ownership errors:

- Ensure migrations own their data (use `String`, not `&str`)
- Clone data if needed: `.to_string()` or `.clone()`
- Use `Arc` for shared ownership

### Directory Discovery Not Working

If directory-based discovery fails:

- Verify the `directory` feature is enabled
- Check that migration directories exist
- Ensure each migration has both `up.sql` and `down.sql`
- Directory names should start with numbers for ordering (e.g., `001_name/`)

### Code Migration Compilation Errors

If code-based migrations don't compile:

- Verify `code` feature is enabled in Cargo.toml
- Check that `Box::new()` wraps the correct types
- Ensure async closures return `Pin<Box<dyn Future>>`

## Related Examples

- **[borrowed_migrations](../borrowed_migrations/)** - Non-static lifetimes for borrowed data
- **[basic_usage](../basic_usage/)** - Simple code-based migrations
- **Embedded migrations example** - Compile-time migration embedding with `include_dir!`
