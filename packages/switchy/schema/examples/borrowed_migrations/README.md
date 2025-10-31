# Borrowed Migrations Example

This example demonstrates how to use `switchy_schema` with borrowed data migrations, showing how to work with non-static lifetimes and dynamic migration content.

## What This Example Shows

- Creating migrations with borrowed data and non-static lifetimes
- Working with dynamic migration content
- Handling lifetime parameters in migration definitions
- Advanced migration patterns for complex scenarios

## Key Features

- **Borrowed Data**: Migrations that reference external data with specific lifetimes
- **Dynamic Content**: Migration content that can be generated at runtime
- **Lifetime Management**: Proper handling of Rust lifetime parameters
- **Flexible Patterns**: Support for complex migration scenarios

## Prerequisites

- Rust 1.70 or later
- Understanding of Rust lifetimes and borrowing
- Familiarity with generic lifetime parameters in Rust

## Running the Example

```bash
cargo run --manifest-path packages/switchy/schema/examples/borrowed_migrations/Cargo.toml
```

Or from the example directory:

```bash
cd packages/switchy/schema/examples/borrowed_migrations
cargo run
```

## Expected Output

This will:

1. Create migrations with borrowed data
2. Generate migrations from configuration at runtime
3. Demonstrate lifetime management
4. Display migration metadata and structure

## Migration Structure

The example demonstrates:

- Migrations with explicit lifetime parameters
- Borrowing data from external sources
- Runtime migration content generation
- Advanced Rust patterns for database migrations

## Use Cases

This pattern is ideal for:

- Applications that generate migrations dynamically
- Complex migration scenarios with external data dependencies
- Advanced Rust applications that need precise lifetime control
- Migrations that reference configuration or external resources

## Lifetime Considerations

When working with borrowed migrations:

- Ensure referenced data lives long enough
- Use appropriate lifetime annotations
- Consider using `Arc` or `Rc` for shared data
- Plan for complex lifetime relationships

## Code Walkthrough

### Defining Borrowed Migrations

Migrations with borrowed data use explicit lifetime parameters:

```rust
struct BorrowedMigration<'a> {
    id: &'a str,
    up_sql: &'a str,
    down_sql: Option<&'a str>,
}
```

### Implementing the Migration Trait

The `Migration` trait implementation carries the lifetime through:

```rust
#[async_trait]
impl<'a> Migration<'a> for BorrowedMigration<'a> {
    fn id(&self) -> &str {
        self.id
    }
    // ... other methods
}
```

## Key Concepts

1. **Lifetime Parameters**: The `'a` lifetime parameter ensures borrowed data outlives the migration
2. **Borrowed References**: Using `&str` instead of `String` for zero-copy efficiency
3. **Dynamic Generation**: Migrations can reference configuration or external sources at runtime
4. **Flexibility**: Supports complex scenarios where migration content comes from external sources

## Testing the Example

Run the example and observe:

1. Migrations are created with borrowed string slices
2. The lifetime system ensures data validity
3. No allocations needed for migration definitions

## Troubleshooting

**Issue**: Lifetime errors when compiling

- **Solution**: Ensure all referenced data has a lifetime at least as long as the migration runner

**Issue**: `error: no bin target named 'borrowed_migrations_example'`

- **Solution**: Run from repository root with full manifest path, or `cd` into the example directory first

## Related Examples

- **[basic_usage](../basic_usage/)** - Simpler example with owned data
- **[static_migrations](../static_migrations/)** - Migrations with `'static` lifetime
