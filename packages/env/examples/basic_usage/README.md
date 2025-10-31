# Basic Usage Example

This example demonstrates basic environment variable access using `switchy_env`.

## Summary

A comprehensive demonstration of how to read, parse, and check environment variables using switchy_env's standard API.

## What This Example Demonstrates

- Getting environment variables as strings with `var()`
- Using default values with `var_or()`
- Parsing variables to specific types (u64, usize, bool) with `var_parse()`
- Parsing with defaults using `var_parse_or()`
- Optional parsing with `var_parse_opt()`
- Checking variable existence with `var_exists()`
- Proper error handling for missing and invalid variables

## Prerequisites

- Basic understanding of Rust
- Familiarity with environment variables
- Knowledge of error handling with `Result`

## Running the Example

```bash
cargo run --manifest-path packages/env/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will display:

```
=== Switchy Env Basic Usage Example ===

1. Getting environment variables as strings:
   PATH is set (truncated): /usr/local/bin:/usr/bin:/bin...

2. Getting variables with defaults:
   PORT (defaults to 8080): 8080
   DEBUG (defaults to false): false

3. Parsing environment variables to specific types:
   TIMEOUT as u64: 30
   MAX_CONNECTIONS as usize: 100
   ENABLE_CACHE as bool: true

4. Parsing with defaults:
   WORKERS (defaults to 4): 4
   VERBOSE (defaults to false): false

5. Optional parsing:
   LOG_LEVEL is set to: 3
   UNSET_VAR is not set (this is expected)

6. Checking variable existence:
   PATH exists: true
   NONEXISTENT_VAR exists: false

7. Handling parse errors:
   Expected parse error: Parse error for 'INVALID_NUMBER': invalid digit found in string

=== Example completed successfully! ===
```

## Code Walkthrough

### Getting Variables as Strings

```rust
match var("PATH") {
    Ok(path) => println!("PATH is set: {}", path),
    Err(e) => println!("Error: {}", e),
}
```

The `var()` function retrieves an environment variable as a `String`. It returns a `Result<String, EnvError>`, so you should handle the error case when the variable doesn't exist.

### Using Defaults

```rust
let port = var_or("PORT", "8080");
```

The `var_or()` function provides a convenient way to get a variable with a fallback value. If the variable doesn't exist, it returns the default instead of an error.

### Parsing to Types

```rust
let timeout: u64 = var_parse("TIMEOUT")?;
```

The `var_parse()` function parses environment variables to any type that implements `FromStr`. It returns a `Result` that can fail if:

- The variable doesn't exist
- The value cannot be parsed to the target type

### Parsing with Defaults

```rust
let workers: usize = var_parse_or("WORKERS", 4);
```

The `var_parse_or()` function combines parsing with a default value. If the variable doesn't exist or fails to parse, it returns the provided default.

### Optional Parsing

```rust
match var_parse_opt::<u32>("LOG_LEVEL") {
    Ok(Some(level)) => println!("LOG_LEVEL is set to: {}", level),
    Ok(None) => println!("LOG_LEVEL is not set"),
    Err(e) => println!("Error parsing LOG_LEVEL: {}", e),
}
```

The `var_parse_opt()` function is useful when a variable is optional. It returns:

- `Ok(Some(value))` if the variable exists and parses successfully
- `Ok(None)` if the variable doesn't exist
- `Err(EnvError::ParseError)` if the variable exists but can't be parsed

### Checking Existence

```rust
if var_exists("PATH") {
    // Variable exists
}
```

The `var_exists()` function checks if a variable is set without retrieving its value.

## Key Concepts

### Type Safety

All parsing functions are type-safe and work with any type implementing `FromStr`. The compiler ensures you handle parse errors appropriately.

### Error Handling

The library uses `EnvError` to distinguish between different error cases:

- `EnvError::NotFound` - Variable doesn't exist
- `EnvError::ParseError` - Variable exists but can't be parsed

### Default Values

Default value functions (`var_or`, `var_parse_or`) are infallible and always return a value, making them ideal for optional configuration.

## Testing the Example

Try running the example with different environment variables set:

```bash
# Set custom values
PORT=3000 DEBUG=true cargo run --manifest-path packages/env/examples/basic_usage/Cargo.toml

# Run with minimal environment
env -i PATH=$PATH cargo run --manifest-path packages/env/examples/basic_usage/Cargo.toml
```

## Troubleshooting

### Parse Errors

If you get parse errors, ensure the environment variable value matches the expected type format:

- `u64`, `usize`: Must be a valid non-negative integer
- `bool`: Must be "true" or "false"
- `i32`: Can be negative integers

### Variable Not Found

If you get "not found" errors, check that:

- The variable name is spelled correctly
- The variable is set in your environment
- You have permission to access the variable

## Related Examples

- `simulator_testing` - Demonstrates using simulator mode for testing
- `custom_provider` - Shows how to implement custom environment providers
