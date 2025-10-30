# Borrowed Migrations Example

This example demonstrates how to use `switchy_schema` with borrowed data migrations, showing how to work with non-static lifetimes and dynamic migration content.

## What This Example Demonstrates

- Creating migrations with borrowed data and non-static lifetimes
- Working with dynamic migration content
- Handling lifetime parameters in migration definitions
- Advanced migration patterns for complex scenarios

## Prerequisites

- Strong understanding of Rust lifetimes and borrowing
- Familiarity with async trait bounds and lifetime constraints
- Experience with `switchy_schema` basic patterns
- Knowledge of when non-static lifetimes are beneficial

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/borrowed_migrations/Cargo.toml
```

This will:

1. Create migrations with borrowed data
2. Generate migrations from configuration at runtime
3. Demonstrate lifetime management
4. Display migration metadata and structure

## Expected Output

```
Borrowed Migrations Example
===========================

✅ Created migrations with borrowed data
📋 Migration definitions:
  - 001_borrowed_migration: Uses borrowed SQL from external source
  - 002_dynamic_migration: Generated from runtime configuration

🎉 Successfully demonstrated:
   • Non-static lifetime management
   • Borrowed data in migrations
   • Dynamic content generation
   • Advanced Rust patterns
```

## Code Walkthrough

### 1. Define Migration with Borrowed Data

Create migrations that borrow from external sources:

```rust
struct BorrowedMigration<'a> {
    id: &'a str,
    up_sql: &'a str,
    down_sql: Option<&'a str>,
}

#[async_trait]
impl<'a> Migration<'a> for BorrowedMigration<'a> {
    fn id(&self) -> &str { self.id }

    async fn up(&self, db: &dyn Database) -> Result<()> {
        db.exec_raw(self.up_sql).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> Result<()> {
        if let Some(sql) = self.down_sql {
            db.exec_raw(sql).await?;
        }
        Ok(())
    }
}
```

### 2. Use Borrowed Data

Reference external configuration or data:

```rust
let config = load_config(); // Returns borrowed data
let migration = BorrowedMigration {
    id: &config.migration_id,
    up_sql: &config.up_sql,
    down_sql: config.down_sql.as_deref(),
};
```

## Key Concepts

### Non-Static Lifetimes

The `Migration` trait supports non-`'static` lifetimes:

```rust
pub trait Migration<'a>: Send + Sync {
    // Allows borrowing data with lifetime 'a
}
```

This enables migrations to reference external data without ownership.

### When to Use Borrowed Migrations

Use borrowed migrations when:

- Loading migration content from configuration files at runtime
- Generating migrations from templates
- Avoiding unnecessary string allocations
- Working with large migration definitions stored elsewhere

### Lifetime Constraints

When working with borrowed migrations, ensure:

- Referenced data outlives the migration usage
- Proper lifetime annotations on structs and implementations
- Async trait bounds include lifetime parameters

## Testing the Example

This example is primarily educational, demonstrating the pattern rather than running actual database migrations. To test in your own code:

1. Load configuration or external data
2. Create borrowed migrations referencing that data
3. Pass to `MigrationRunner` as usual
4. Ensure borrowed data lives long enough

## Troubleshooting

### Lifetime Error: "borrowed value does not live long enough"

If you get lifetime errors:

- Ensure borrowed data outlives migration usage
- Consider using `Arc<String>` or `'static` if data needs to outlive scope
- Review lifetime annotations on your migration structs

### Error: "cannot infer an appropriate lifetime"

This occurs when Rust can't determine lifetimes automatically:

- Add explicit lifetime annotations to structs
- Use lifetime bounds on trait implementations
- Consider simplifying lifetime relationships

### When to Use Static Migrations Instead

Use `'static` lifetimes (like in `basic_usage` example) when:

- Migrations own their data
- Content is known at compile time
- Simpler lifetime management is preferred
- No need for runtime-generated content

## Related Examples

- **[static_migrations](../static_migrations/)** - Migrations with owned data (`'static` lifetime)
- **[basic_usage](../basic_usage/)** - Standard code-based migrations
