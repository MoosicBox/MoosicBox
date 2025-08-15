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

## Running the Example

```bash
cargo run --bin borrowed_migrations_example
```

This will:
1. Create migrations with borrowed data
2. Apply them to an in-memory SQLite database
3. Demonstrate lifetime management
4. Display the migration results

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