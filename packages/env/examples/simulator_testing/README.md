# Simulator Testing Example

Demonstrates how to use switchy_env's simulator mode for deterministic, reproducible testing of environment-dependent code.

## Summary

This example shows how the simulator feature provides a controlled environment for testing, with predefined defaults and the ability to set, modify, and reset variables programmatically.

## What This Example Demonstrates

- Accessing predefined simulator default values
- Setting custom environment variables for test scenarios
- Removing individual variables with `remove_var()`
- Resetting to defaults with `reset()`
- Clearing all variables with `clear()`
- Testing configuration loading under different scenarios
- Creating reproducible test environments

## Prerequisites

- Understanding of environment variables and configuration management
- Basic knowledge of Rust testing patterns
- Familiarity with the basic switchy_env API (see `basic_usage` example)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml
```

## Expected Output

```
=== switchy_env Simulator Testing Example ===

1. Default simulator environment:
   DATABASE_URL = sqlite::memory:
   PORT = 8080
   SIMULATOR_SEED = 12345
   DEBUG_RENDERER = 0

2. Loading app configuration with defaults:
   AppConfig { port: 8080, database_url: "sqlite::memory:", debug: false, max_connections: 1 }

3. Setting custom test values:
   PORT = 9000
   DATABASE_URL = postgresql://test:test@localhost/testdb
   DEBUG_RENDERER = 1
   CUSTOM_VAR = custom_value

4. Loading config with custom values:
   AppConfig { port: 9000, database_url: "postgresql://test:test@localhost/testdb", debug: true, max_connections: 1 }

5. Removing CUSTOM_VAR:
   CUSTOM_VAR exists: false

6. Resetting to defaults:
   PORT = 8080
   DATABASE_URL = sqlite::memory:
   CUSTOM_VAR exists: false

7. Clearing all variables:
   PORT exists: false
   DATABASE_URL exists: false
   Expected error: Environment variable 'PORT' not found

8. Setting up a test scenario:
   Production-like config: AppConfig { port: 443, database_url: "postgresql://prod:pass@db.example.com/prod", debug: false, max_connections: 10 }

9. Testing multiple scenarios:
   Development config: AppConfig { port: 3000, database_url: "sqlite::memory:", debug: true, max_connections: 1 }
   Test config: AppConfig { port: 8080, database_url: "sqlite::memory:", debug: true, max_connections: 1 }

=== Example Complete ===

Key takeaway: The simulator allows you to test your code with
different environment configurations in a controlled, reproducible way.
```

## Code Walkthrough

### 1. Simulator Defaults

The simulator comes with predefined defaults for common variables:

```rust
// These are automatically set when using simulator mode:
DATABASE_URL = "sqlite::memory:"
PORT = "8080"
SIMULATOR_SEED = "12345"
DEBUG_RENDERER = "0"
// ... and more
```

These defaults make it easy to write tests without setting up environment variables.

### 2. Setting Test Values

```rust
use switchy_env::simulator::set_var;

set_var("PORT", "9000");
set_var("DATABASE_URL", "postgresql://test:test@localhost/testdb");
```

The `set_var()` function is only available in simulator mode. It allows you to programmatically configure the environment for testing.

### 3. Removing Variables

```rust
use switchy_env::simulator::remove_var;

remove_var("CUSTOM_VAR");
```

Remove a specific variable from the simulator environment. This is useful for testing code that handles missing variables.

### 4. Resetting to Defaults

```rust
use switchy_env::simulator::reset;

reset();
```

The `reset()` function restores all variables to their default values, including:

- System environment variables (from the real environment)
- Simulator-specific defaults

This is perfect for cleaning up between test scenarios.

### 5. Clearing All Variables

```rust
use switchy_env::simulator::clear;

clear();
```

The `clear()` function removes ALL variables, creating a completely empty environment. Use this to test error handling for missing variables.

### 6. Testing Configuration Loading

```rust
#[derive(Debug)]
struct AppConfig {
    port: u16,
    database_url: String,
    debug: bool,
}

impl AppConfig {
    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            port: var_parse("PORT")?,
            database_url: var("DATABASE_URL")?,
            debug: var_parse("DEBUG_RENDERER")?,
        })
    }
}
```

Create configuration types that load from the environment, then test them with different simulator setups.

## Key Concepts

### Simulator vs. Standard Mode

The simulator mode is enabled with the `simulator` feature (default in switchy_env). When active:

- Variables start with predefined defaults
- You can modify variables with `set_var()`, `remove_var()`, etc.
- The environment is isolated and deterministic
- Real system environment variables are still available as a base

### Deterministic Testing

The simulator provides deterministic defaults (like `SIMULATOR_SEED = "12345"`), making tests reproducible:

```rust
// These tests will always see the same environment
#[test]
fn test_default_config() {
    reset(); // Start with known state
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.port, 8080);
}
```

### Test Scenario Patterns

Pattern for testing multiple scenarios:

```rust
// Test 1: Development environment
reset();
set_var("PORT", "3000");
set_var("DEBUG", "1");
test_development_behavior();

// Test 2: Production environment
reset();
set_var("PORT", "443");
set_var("DEBUG", "0");
test_production_behavior();
```

### When to Use Simulator Mode

Use the simulator mode when:

- Writing unit tests that depend on environment variables
- Testing configuration loading logic
- Simulating different deployment environments
- Ensuring deterministic test behavior
- Testing error handling for missing/invalid variables

Use standard mode when:

- Running in production
- You need real environment variable access
- Building CLI tools that respect user's environment

## Testing the Example

You can modify the example to test different scenarios:

1. **Test custom defaults**: Modify the default values in the simulator to see how your app behaves
2. **Test missing variables**: Use `clear()` and selectively set variables to test error handling
3. **Test invalid values**: Set variables to invalid values to verify parse error handling
4. **Test scenario transitions**: Practice resetting and reconfiguring for different test cases

## Troubleshooting

### Variables Not Resetting

If variables aren't resetting as expected:

- Call `reset()` at the start of each test scenario
- Remember that `reset()` restores defaults, while `clear()` removes everything
- Check that you're using `switchy_env::simulator::set_var`, not `std::env::set_var`

### Feature Configuration

Ensure the `simulator` feature is enabled in your `Cargo.toml`:

```toml
[dependencies]
switchy_env = { workspace = true }  # simulator is default
```

Or explicitly:

```toml
[dependencies]
switchy_env = { workspace = true, features = ["simulator"] }
```

### Conflicts with std::env

Don't mix simulator functions with `std::env` functions in tests:

```rust
// Don't do this:
std::env::set_var("PORT", "3000");  // Won't affect simulator
let port = switchy_env::var("PORT")?;  // Reads from simulator

// Do this instead:
switchy_env::simulator::set_var("PORT", "3000");  // Sets in simulator
let port = switchy_env::var("PORT")?;  // Reads from simulator
```

## Related Examples

- **basic_usage**: Learn the fundamental environment variable access API
- **custom_provider**: Implement custom environment variable sources beyond the simulator
