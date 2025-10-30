# Basic Usage Example

A comprehensive demonstration of environment variable access patterns using switchy_env in standard mode.

## Summary

This example shows how to read, parse, and handle environment variables with proper error handling and default values.

## What This Example Demonstrates

- Reading environment variables as strings with `var()`
- Using default values with `var_or()`
- Parsing variables to specific types with `var_parse()`
- Using parsed defaults with `var_parse_or()`
- Handling optional variables with `var_parse_opt()`
- Checking variable existence with `var_exists()`
- Proper error handling for missing and invalid variables

## Prerequisites

- Basic understanding of environment variables
- Familiarity with Rust's `Result` type for error handling
- Knowledge of the `FromStr` trait for type parsing

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/env/examples/basic_usage/Cargo.toml
```

The example sets its own test variables internally for demonstration purposes, so you don't need to configure any environment variables beforehand.

## Expected Output

```
=== switchy_env Basic Usage Example ===

1. Reading variables as strings:
   PORT = 8080

2. Using default values:
   HOST = localhost (using default)
   PORT = 8080 (from environment)

3. Parsing to specific types:
   PORT as u16 = 8080
   DEBUG as bool = true

4. Parsing with defaults:
   MAX_CONNECTIONS = 100 (from environment)
   BUFFER_SIZE = 4096 (using default)

5. Optional variables:
   TIMEOUT_SECS = 30 seconds
   RETRY_COUNT not set (will use default)

6. Checking variable existence:
   PORT exists: true
   NONEXISTENT exists: false

7. Error handling:
   Expected error: Environment variable 'EXAMPLE_MISSING_VAR' not found
   Expected parse error: Parse error for 'EXAMPLE_INVALID_NUMBER': invalid digit found in string

=== Example Complete ===
```

## Code Walkthrough

### 1. Basic String Access

```rust
match var("EXAMPLE_PORT") {
    Ok(port) => println!("PORT = {port}"),
    Err(e) => println!("Error: {e}"),
}
```

The `var()` function returns a `Result<String, EnvError>`. Use pattern matching or the `?` operator for error handling.

### 2. Default Values

```rust
let host = var_or("EXAMPLE_HOST", "localhost");
```

The `var_or()` function provides a fallback value if the variable doesn't exist. This never fails - it always returns a `String`.

### 3. Type Parsing

```rust
let port: u16 = var_parse("EXAMPLE_PORT")?;
```

The `var_parse()` function reads the variable and parses it to the specified type. It works with any type implementing `FromStr`:

- Numbers: `u16`, `u32`, `u64`, `i32`, `f64`, etc.
- Boolean: `bool` (accepts "true"/"false", "1"/"0", "yes"/"no")
- Other types: `IpAddr`, `SocketAddr`, or custom types with `FromStr`

### 4. Parsed Defaults

```rust
let buffer_size: usize = var_parse_or("EXAMPLE_BUFFER_SIZE", 4096);
```

Combines parsing with default values - if the variable is missing or can't be parsed, returns the default.

### 5. Optional Variables

```rust
match var_parse_opt::<u64>("EXAMPLE_TIMEOUT_SECS") {
    Ok(Some(timeout)) => println!("TIMEOUT_SECS = {timeout}"),
    Ok(None) => println!("TIMEOUT_SECS not set"),
    Err(e) => println!("Parse error: {e}"),
}
```

The `var_parse_opt()` function distinguishes between:

- `Ok(Some(value))`: Variable exists and parsed successfully
- `Ok(None)`: Variable doesn't exist (not an error)
- `Err(ParseError)`: Variable exists but couldn't be parsed (this is an error)

This is useful when you need to know whether a variable was explicitly set.

### 6. Existence Checking

```rust
if var_exists("EXAMPLE_PORT") {
    // Variable is set
}
```

Use `var_exists()` to check if a variable is set without reading its value.

## Key Concepts

### Error Types

The `switchy_env::EnvError` enum has three variants:

- `NotFound(String)`: Variable doesn't exist
- `InvalidValue(String, String)`: Variable has an invalid format
- `ParseError(String, String)`: Failed to parse to target type

### Type Safety

All parsing functions use Rust's type system to ensure safety:

```rust
let port: u16 = var_parse("PORT")?;  // Type inference
let port = var_parse::<u16>("PORT")?; // Explicit type
```

Both forms work - use whichever is clearer in context.

### When to Use Each Function

- `var()`: When you need the raw string value
- `var_or()`: When you always want a string, with a sensible default
- `var_parse()`: When the variable must exist and be valid
- `var_parse_or()`: When you want a typed value with a default
- `var_parse_opt()`: When you need to distinguish "not set" from "invalid"
- `var_exists()`: When you only care if the variable is defined

## Testing the Example

You can modify the example to test different scenarios:

1. **Test different types**: Change the parsing types to experiment with different data types
2. **Test invalid values**: Set variables to invalid values to see parse errors
3. **Test missing variables**: Remove the `set_var()` calls to see `NotFound` errors
4. **Use real environment**: Comment out the `set_var()` calls and set actual environment variables

Example with real environment variables:

```bash
EXAMPLE_PORT=3000 EXAMPLE_DEBUG=false cargo run --manifest-path packages/env/examples/basic_usage/Cargo.toml
```

## Troubleshooting

### Parse Errors

If you get parse errors, verify that:

- The variable value matches the expected format for the type
- For booleans, use "true"/"false", "1"/"0", or "yes"/"no"
- For numbers, ensure there are no spaces or non-numeric characters

### Type Inference Issues

If you get type inference errors:

```rust
// This might fail:
let value = var_parse("VAR")?;

// Be explicit:
let value: u32 = var_parse("VAR")?;
// or
let value = var_parse::<u32>("VAR")?;
```

## Related Examples

- **simulator_testing**: Learn how to use the simulator mode for deterministic testing
- **custom_provider**: Implement custom environment variable sources
