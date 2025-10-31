# Simulator Testing Example

This example demonstrates using `switchy_env` in simulator mode for testing.

## Summary

A comprehensive guide to using switchy_env's simulator mode to create deterministic, isolated test environments without affecting system environment variables.

## What This Example Demonstrates

- Accessing deterministic default values provided by the simulator
- Setting custom environment variables for testing with `set_var()`
- Removing variables with `remove_var()`
- Resetting to defaults for test isolation with `reset()`
- Clearing all variables with `clear()`
- Practical testing patterns for test setup and teardown

## Prerequisites

- Basic understanding of Rust
- Familiarity with unit testing concepts
- Knowledge of environment variables

## Running the Example

Run without simulator features (shows defaults):

```bash
cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml
```

Run with simulator features enabled (full functionality):

```bash
cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml --features simulator
```

## Expected Output

With simulator features enabled, you'll see:

```
=== Switchy Env Simulator Testing Example ===

1. Accessing simulator defaults:
   These are deterministic values set by the simulator for testing

   SIMULATOR_SEED: 12345
   SIMULATOR_UUID_SEED: 54321
   SIMULATOR_EPOCH_OFFSET: 0
   DATABASE_URL: sqlite::memory:
   DB_HOST: localhost
   PORT: 8080
   SSL_PORT: 8443

2. Setting custom variables for testing:
   TEST_API_KEY: test_key_12345
   TEST_ENDPOINT: http://localhost:8080/api

3. Removing variables:
   Before removal: TEST_API_KEY exists = true
   After removal: TEST_API_KEY exists = false

4. Demonstrating test isolation:
   Setting up test environment...
   TEST_MODE: integration
   PORT (overridden): 9999

   Resetting environment to defaults...
   After reset:
   TEST_MODE exists: false
   PORT (back to default): 8080

5. Clearing all variables:
   Variables before clear:
   - DATABASE_URL exists: true
   - PORT exists: true

   Variables after clear:
   - DATABASE_URL exists: false
   - PORT exists: false

   After reset, variables are restored:
   - DATABASE_URL exists: true

6. Practical testing pattern:
   This demonstrates how you might structure a test:

   Test values:
   - DB_CONNECTION: test_connection
   - CACHE_ENABLED: true

   Environment reset for next test

=== Example completed successfully! ===
```

## Code Walkthrough

### Accessing Simulator Defaults

```rust
let simulator_seed = var("SIMULATOR_SEED")?;
println!("SIMULATOR_SEED: {}", simulator_seed); // Always "12345"
```

The simulator provides deterministic default values for common configuration variables. These defaults ensure tests run consistently across different environments.

### Setting Variables for Testing

```rust
switchy_env::set_var("TEST_API_KEY", "test_key_12345");
let api_key = var("TEST_API_KEY")?;
```

Unlike `std::env::set_var`, simulator's `set_var()` only affects the simulator's internal state, not the actual system environment. This provides isolation between tests.

### Removing Variables

```rust
switchy_env::remove_var("TEST_API_KEY");
```

The `remove_var()` function removes a variable from the simulator. This is useful for testing error handling when required variables are missing.

### Resetting to Defaults

```rust
switchy_env::reset();
```

The `reset()` function clears all variables and restores simulator defaults. This is essential for test isolation - each test can start with a known, clean state.

### Clearing All Variables

```rust
switchy_env::clear();
```

The `clear()` function removes all variables, including defaults. This is useful for testing behavior when the environment is completely empty.

## Key Concepts

### Test Isolation

The simulator mode enables proper test isolation by:

- Not modifying the actual system environment
- Providing a `reset()` function to restore known state
- Allowing independent test execution without side effects

### Deterministic Defaults

The simulator provides consistent default values:

- `SIMULATOR_SEED`: "12345"
- `DATABASE_URL`: "sqlite::memory:"
- `PORT`: "8080"
- `DB_HOST`: "localhost"

These defaults make tests predictable and reproducible.

### Practical Testing Pattern

A typical test structure:

```rust
#[test]
fn test_database_config() {
    // Setup: Reset to clean state
    switchy_env::reset();
    switchy_env::set_var("DB_URL", "sqlite::test.db");

    // Test: Verify behavior
    let config = load_database_config();
    assert_eq!(config.url, "sqlite::test.db");

    // Teardown: Reset for next test
    switchy_env::reset();
}
```

### Feature-Gated Code

The example uses conditional compilation for simulator-specific features:

```rust
#[cfg(feature = "simulator")]
{
    switchy_env::set_var("KEY", "value");
}
```

This allows the code to work with or without the simulator feature enabled.

## Testing the Example

Try different scenarios:

**Test with empty environment:**

```bash
cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml --features simulator
```

**Compare with standard mode:**

```bash
# Run without simulator features to see the difference
cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml
```

## Troubleshooting

### Simulator Features Not Working

If you see "(Simulator mode not enabled)", ensure you're running with:

```bash
--features simulator
```

### Variables Persisting Between Tests

Always call `reset()` at the beginning of each test to ensure a clean state.

### Default Values Not Present

If expected defaults are missing:

- Ensure the simulator feature is enabled
- Call `reset()` to restore defaults after calling `clear()`

## Related Examples

- `basic_usage` - Demonstrates standard environment variable access
- `custom_provider` - Shows how to implement custom environment providers
