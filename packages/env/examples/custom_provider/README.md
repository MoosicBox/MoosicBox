# Custom Provider Example

This example demonstrates how to implement custom `EnvProvider` trait implementations for alternative environment variable sources.

## Summary

A comprehensive guide to creating custom environment providers including layered configuration (with precedence), prefixed/namespaced providers, and advanced provider patterns.

## What This Example Demonstrates

- Implementing the `EnvProvider` trait for custom sources
- Creating a layered provider with override, system, and default levels
- Building a prefixed provider for namespaced configuration
- Using all `EnvProvider` methods (`var_parse`, `var_or`, etc.) with custom providers
- Error handling with custom providers
- Listing all variables from custom sources

## Prerequisites

- Intermediate understanding of Rust
- Familiarity with traits and trait implementation
- Understanding of `BTreeMap` and collections
- Knowledge of environment variable concepts

## Running the Example

```bash
cargo run --manifest-path packages/env/examples/custom_provider/Cargo.toml
```

## Expected Output

```
=== Switchy Env Custom Provider Example ===

1. Layered Environment Provider:
   Combines overrides, system environment, and defaults

   Defaults:
   - APP_NAME: MyApp
   - APP_VERSION: 1.0.0
   - LOG_LEVEL: info

   After setting LOG_LEVEL=debug in system env:
   - LOG_LEVEL: debug

   After adding override LOG_LEVEL=trace:
   - LOG_LEVEL: trace

2. Type Parsing with Custom Provider:
   MAX_WORKERS (parsed as usize): 4
   TIMEOUT_SECONDS (parsed as u64): 30

3. Prefixed Environment Provider:
   Automatically adds namespace prefix to variable names

   Accessing 'DATABASE' (actually reads 'MYAPP_DATABASE'):
   - DATABASE: postgres://localhost/mydb

   Accessing 'PORT' (actually reads 'MYAPP_PORT'):
   - PORT: 3000

   Accessing 'ENABLE_CACHE' (actually reads 'MYAPP_ENABLE_CACHE'):
   - ENABLE_CACHE: true

4. Listing All Variables:

   All layered provider variables:
   - APP_NAME = MyApp
   - APP_VERSION = 1.0.0
   - LOG_LEVEL = trace
   - MAX_WORKERS = 4
   - TIMEOUT_SECONDS = 30
   ... (X total variables)

   All prefixed provider variables (MYAPP_*):
   - DATABASE = postgres://localhost/mydb
   - ENABLE_CACHE = true
   - PORT = 3000

5. Default Values with Custom Providers:
   DB_POOL_SIZE (with default): 10
   CACHE_TTL (with default): 300

6. Error Handling:
   Variable 'NONEXISTENT_VAR' not found (expected)

=== Example completed successfully! ===
```

## Code Walkthrough

### Implementing EnvProvider

The `EnvProvider` trait requires implementing two methods:

```rust
impl EnvProvider for LayeredEnvProvider {
    fn var(&self, name: &str) -> Result<String> {
        // Return variable value or NotFound error
    }

    fn vars(&self) -> BTreeMap<String, String> {
        // Return all variables as a map
    }
}
```

All other methods (`var_parse`, `var_or`, etc.) have default implementations that build on these two methods.

### Layered Provider Pattern

The `LayeredEnvProvider` demonstrates a common configuration pattern with multiple layers:

```rust
fn var(&self, name: &str) -> Result<String> {
    // 1. Check overrides (highest priority)
    if let Some(value) = self.overrides.get(name) {
        return Ok(value.clone());
    }

    // 2. Check system environment
    if let Ok(value) = std::env::var(name) {
        return Ok(value);
    }

    // 3. Check defaults (lowest priority)
    if let Some(value) = self.defaults.get(name) {
        return Ok(value.clone());
    }

    Err(EnvError::NotFound(name.to_string()))
}
```

This pattern is useful for:

- Providing application-specific defaults
- Allowing system environment overrides
- Supporting runtime configuration changes via overrides

### Prefixed Provider Pattern

The `PrefixedEnvProvider` demonstrates namespacing:

```rust
fn var(&self, name: &str) -> Result<String> {
    let prefixed = format!("{}_{}", self.prefix, name);
    std::env::var(&prefixed)
        .map_err(|_| EnvError::NotFound(name.to_string()))
}
```

This allows code to access variables by simple names while maintaining a namespace in the actual environment:

- Code asks for `"DATABASE"`
- Provider looks up `"MYAPP_DATABASE"`
- Prevents naming conflicts between applications

### Using Custom Providers

Once implemented, custom providers work with all `EnvProvider` methods:

```rust
let provider = LayeredEnvProvider::new();

// String access
let name = provider.var("APP_NAME")?;

// Parsing
let workers: usize = provider.var_parse("MAX_WORKERS")?;

// Defaults
let pool_size = provider.var_or("DB_POOL_SIZE", "10");

// Optional parsing
let cache_ttl: Option<u32> = provider.var_parse_opt("CACHE_TTL")?;
```

## Key Concepts

### Trait-Based Abstraction

The `EnvProvider` trait provides a common interface for different environment sources:

- Standard system environment (`StandardEnv`)
- Simulator for testing (`SimulatorEnv`)
- Custom sources (files, databases, remote config, etc.)

### Precedence and Layering

Layered configuration allows sophisticated precedence rules:

1. **Overrides** - Runtime changes (highest priority)
2. **System Environment** - User configuration
3. **Defaults** - Application defaults (lowest priority)

This pattern is common in enterprise applications.

### Namespacing

Prefixing prevents variable name conflicts:

- Multiple applications can run with different namespaces
- Clearer organization of configuration
- Easier to identify which application owns which variables

### Type Safety

Custom providers maintain type safety through the `EnvProvider` trait:

- Compile-time type checking
- Parse errors are caught and reported
- No runtime type confusion

## Testing the Example

Try experimenting with different configurations:

```bash
# Run with custom environment variables
APP_NAME=CustomApp LOG_LEVEL=warn cargo run --manifest-path packages/env/examples/custom_provider/Cargo.toml

# Test prefixed variables
MYAPP_DATABASE=sqlite::memory: cargo run --manifest-path packages/env/examples/custom_provider/Cargo.toml
```

## Troubleshooting

### Variable Not Found Errors

If you get `NotFound` errors:

- Check that the variable name is correct
- Verify prefixes match (for `PrefixedEnvProvider`)
- Ensure defaults are set up correctly (for `LayeredEnvProvider`)

### Type Parse Errors

If parsing fails:

- Verify the variable value matches the expected type format
- Check that the variable actually contains a parseable value
- Use `var()` first to see the raw string value

### Trait Implementation Issues

When implementing `EnvProvider`:

- Ensure both `var()` and `vars()` are implemented
- Mark your provider as `Send + Sync` if using in concurrent code
- Return `EnvError::NotFound` for missing variables
- Return `EnvError::ParseError` for parse failures (handled automatically by default methods)

## Related Examples

- `basic_usage` - Basic environment variable access patterns
- `simulator_testing` - Testing with simulator mode
