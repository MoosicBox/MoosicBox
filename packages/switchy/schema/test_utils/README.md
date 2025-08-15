# Switchy Schema Test Utils

Test utilities for `switchy_schema` that provide comprehensive migration testing capabilities.

## Overview

This package provides three main testing utilities for validating database migrations:

- `verify_migrations_full_cycle` - Basic up/down migration testing
- `verify_migrations_with_state` - Migration testing with data preservation
- `verify_migrations_with_mutations` - Comprehensive testing with various database states

## Features

- **Multiple Database Support**: SQLite, PostgreSQL, MySQL support via feature flags
- **Comprehensive Testing**: Full migration lifecycle validation
- **State Preservation**: Verify data integrity during migrations
- **Mutation Testing**: Test against various database states and scenarios
- **Async Support**: Full async/await support for modern Rust applications

## Usage

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
switchy_schema_test_utils = { workspace = true, features = ["sqlite"] }
```

### Basic Migration Testing

```rust
use switchy_schema_test_utils::verify_migrations_full_cycle;

#[tokio::test]
async fn test_migrations() {
    let migrations = vec![/* your migrations */];
    verify_migrations_full_cycle(&migrations).await.unwrap();
}
```

### State Preservation Testing

```rust
use switchy_schema_test_utils::verify_migrations_with_state;

#[tokio::test]
async fn test_migrations_with_data() {
    let migrations = vec![/* your migrations */];
    verify_migrations_with_state(
        &migrations,
        setup_state,
        validate_state,
    ).await.unwrap();
}
```

### Mutation Testing

```rust
use switchy_schema_test_utils::verify_migrations_with_mutations;

#[tokio::test]
async fn test_migrations_comprehensive() {
    let migrations = vec![/* your migrations */];
    let mutation_provider = MyMutationProvider::new();
    verify_migrations_with_mutations(
        &migrations,
        &mutation_provider,
    ).await.unwrap();
}
```

## Features

- `sqlite` - Enable SQLite support (default)
- `postgres` - Enable PostgreSQL support
- `mysql` - Enable MySQL support

## Examples

See the `examples/` directory in the parent `switchy_schema` package for complete working examples of each testing utility.