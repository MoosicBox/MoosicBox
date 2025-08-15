# Static Migrations Example

This example demonstrates how to use `switchy_schema` with static lifetime migrations, showing the simplest way to define and run database migrations.

## What This Example Shows

- Creating migrations with static string definitions
- Using the embedded migrations feature
- Running migrations programmatically
- Basic migration structure and patterns

## Key Features

- **Static Lifetimes**: Migrations defined with `'static` lifetime for embedded use
- **Embedded Migrations**: Migrations compiled into the binary
- **Simple Setup**: Minimal configuration required
- **Directory-based**: Migrations loaded from filesystem directory

## Running the Example

```bash
cargo run --bin static_migrations_example
```

This will:
1. Load migrations from the embedded directory
2. Apply them to an in-memory SQLite database
3. Display the migration results

## Migration Structure

The example uses:
- Directory-based migration loading
- Static string migrations for embedded deployment
- Simple up/down migration pattern
- Minimal dependencies and configuration

## Use Cases

This pattern is ideal for:
- Simple applications with basic migration needs
- Embedded applications where migrations are compiled in
- Getting started with `switchy_schema`
- Applications that don't need complex migration testing