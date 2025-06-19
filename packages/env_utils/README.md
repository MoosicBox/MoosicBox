# MoosicBox Environment Utils

A utility library for parsing and handling environment variables with type-safe conversions and compile-time macros.

## Features

- **Type-Safe Parsing**: Parse environment variables to specific numeric types (usize, u64, u32, u16, i64, i32, i16, i8, f32)
- **Compile-Time Macros**: Extract environment variables at compile time with default values
- **Const-Compatible Parsing**: Const-friendly integer parsing functions for compile-time evaluation
- **Optional Values**: Handle missing environment variables gracefully with Option types
- **Error Handling**: Proper error types for parsing failures and missing variables

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_env_utils = "0.1.1"
```

## Usage

### Basic Environment Variable Parsing

```rust
use moosicbox_env_utils::{env_usize, default_env_usize, option_env_usize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse required environment variable
    let port = env_usize("PORT")?;
    println!("Server port: {}", port);

    // Parse with default value
    let timeout = default_env_usize("TIMEOUT", 30)?;
    println!("Timeout: {} seconds", timeout);

    // Parse optional environment variable
    match option_env_usize("MAX_CONNECTIONS")? {
        Some(max_conn) => println!("Max connections: {}", max_conn),
        None => println!("No connection limit set"),
    }

    Ok(())
}
```

### Compile-Time Environment Variable Macros

```rust
use moosicbox_env_utils::{env_usize, default_env_usize, default_env_u64, default_env_u32};

// Extract environment variables at compile time
const SERVER_PORT: usize = env_usize!("PORT");
const MAX_BUFFER_SIZE: usize = default_env_usize!("BUFFER_SIZE", 8192);
const CACHE_TTL: u64 = default_env_u64!("CACHE_TTL", 3600);
const WORKER_THREADS: u32 = default_env_u32!("WORKERS", 4);

fn main() {
    println!("Server will run on port: {}", SERVER_PORT);
    println!("Buffer size: {} bytes", MAX_BUFFER_SIZE);
    println!("Cache TTL: {} seconds", CACHE_TTL);
    println!("Worker threads: {}", WORKER_THREADS);
}
```

### Different Numeric Types

```rust
use moosicbox_env_utils::{
    option_env_u64, option_env_u32, option_env_u16,
    option_env_i64, option_env_i32, option_env_i16, option_env_i8,
    option_env_f32
};

async fn configure_application() -> Result<(), Box<dyn std::error::Error>> {
    // Unsigned integers
    let memory_limit: Option<u64> = option_env_u64("MEMORY_LIMIT_MB")?;
    let max_requests: Option<u32> = option_env_u32("MAX_REQUESTS")?;
    let port: Option<u16> = option_env_u16("PORT")?;

    // Signed integers
    let timezone_offset: Option<i64> = option_env_i64("TIMEZONE_OFFSET")?;
    let priority: Option<i32> = option_env_i32("PROCESS_PRIORITY")?;
    let thread_priority: Option<i16> = option_env_i16("THREAD_PRIORITY")?;
    let log_level: Option<i8> = option_env_i8("LOG_LEVEL")?;

    // Floating point
    let cpu_threshold: Option<f32> = option_env_f32("CPU_THRESHOLD")?;

    println!("Configuration loaded:");
    if let Some(mem) = memory_limit {
        println!("  Memory limit: {} MB", mem);
    }
    if let Some(reqs) = max_requests {
        println!("  Max requests: {}", reqs);
    }
    if let Some(threshold) = cpu_threshold {
        println!("  CPU threshold: {:.2}%", threshold * 100.0);
    }

    Ok(())
}
```

### String Environment Variables

```rust
use moosicbox_env_utils::default_env;

fn main() {
    // Get string environment variable with default
    let app_name = default_env("APP_NAME", "MoosicBox");
    let environment = default_env("ENVIRONMENT", "development");

    println!("Application: {} ({})", app_name, environment);
}
```

### Const Integer Parsing

```rust
use moosicbox_env_utils::{parse_usize, parse_isize};

const fn compile_time_parsing() -> usize {
    // These functions work at compile time
    match parse_usize("12345") {
        Ok(value) => value,
        Err(_) => 0,
    }
}

const PARSED_VALUE: usize = compile_time_parsing();

fn main() {
    println!("Parsed at compile time: {}", PARSED_VALUE);

    // Also works at runtime
    let runtime_value = parse_isize("-42").unwrap();
    println!("Parsed at runtime: {}", runtime_value);
}
```

## API Reference

### Runtime Functions

- `env_usize(name)` - Parse required usize environment variable
- `default_env_usize(name, default)` - Parse usize with fallback default
- `option_env_*` functions - Parse optional values for various types
- `default_env(name, default)` - Get string environment variable with default

### Compile-Time Macros

- `env_usize!(name)` - Extract required usize at compile time
- `default_env_usize!(name, default)` - Extract usize with default at compile time
- `default_env_u64!(name, default)` - Extract u64 with default at compile time
- `default_env_u32!(name, default)` - Extract u32 with default at compile time
- `option_env_*!` macros - Extract optional values at compile time

### Const Functions

- `parse_usize(s)` - Parse string to usize (const-compatible)
- `parse_isize(s)` - Parse string to isize with sign support (const-compatible)

## Error Handling

The library provides specific error types for different failure scenarios:

- `EnvUsizeError` - Environment variable missing or parsing failed
- `DefaultEnvUsizeError` - Parsing failed (missing variables return default)
- `OptionEnvUsizeError` - Parsing failed for numeric types
- `OptionEnvF32Error` - Parsing failed for floating point
- `ParseIntError` - Invalid digit encountered during const parsing

## Performance

- Const functions enable compile-time evaluation
- Macro-based extraction has zero runtime cost
- Runtime parsing uses standard library implementations for reliability
