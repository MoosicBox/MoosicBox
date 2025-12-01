# MoosicBox Environment Utils

A utility library for parsing and handling environment variables with compile-time macros and const-compatible parsing functions.

## Features

- **Compile-Time Macros**: Extract environment variables at compile time with default values and type conversions
- **Type-Safe Parsing**: Support for multiple numeric types (usize, u64, u32, u16, isize, i64, i32, i16, i8)
- **Const-Compatible Parsing**: Const-friendly integer parsing functions for compile-time evaluation
- **Optional Values**: Handle missing environment variables gracefully with Option types
- **Zero Runtime Cost**: Macro-based extraction evaluates at compile time

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_env_utils = "0.1.4"
```

## Usage

### Basic Compile-Time Environment Variable Extraction

```rust
use moosicbox_env_utils::{env_usize, default_env_usize, option_env_usize};

// Extract required environment variable at compile time
const SERVER_PORT: usize = env_usize!("PORT");

// Extract with default value at compile time
const TIMEOUT: usize = default_env_usize!("TIMEOUT", 30);

// Extract optional environment variable at compile time
const MAX_CONNECTIONS: Option<usize> = option_env_usize!("MAX_CONNECTIONS");

fn main() {
    println!("Server port: {}", SERVER_PORT);
    println!("Timeout: {} seconds", TIMEOUT);

    match MAX_CONNECTIONS {
        Some(max_conn) => println!("Max connections: {}", max_conn),
        None => println!("No connection limit set"),
    }
}
```

### Different Integer Types

```rust
use moosicbox_env_utils::{default_env_usize, default_env_u64, default_env_u32, default_env_u16};

// Extract environment variables with different unsigned integer types
const MAX_BUFFER_SIZE: usize = default_env_usize!("BUFFER_SIZE", 8192);
const CACHE_TTL: u64 = default_env_u64!("CACHE_TTL", 3600);
const WORKER_THREADS: u32 = default_env_u32!("WORKERS", 4);
const PORT: u16 = default_env_u16!("PORT", 8080);

fn main() {
    println!("Buffer size: {} bytes", MAX_BUFFER_SIZE);
    println!("Cache TTL: {} seconds", CACHE_TTL);
    println!("Worker threads: {}", WORKER_THREADS);
    println!("Port: {}", PORT);
}
```

### Signed Integer Support

```rust
use moosicbox_env_utils::{
    option_env_u64, option_env_u32, option_env_u16,
    option_env_isize, option_env_i64, option_env_i32, option_env_i16, option_env_i8
};

// Unsigned integers
const MEMORY_LIMIT: Option<u64> = option_env_u64!("MEMORY_LIMIT_MB");
const MAX_REQUESTS: Option<u32> = option_env_u32!("MAX_REQUESTS");
const PORT: Option<u16> = option_env_u16!("PORT");

// Signed integers
const TIMEZONE_OFFSET: Option<i64> = option_env_i64!("TIMEZONE_OFFSET");
const PRIORITY: Option<i32> = option_env_i32!("PROCESS_PRIORITY");
const THREAD_PRIORITY: Option<i16> = option_env_i16!("THREAD_PRIORITY");
const LOG_LEVEL: Option<i8> = option_env_i8!("LOG_LEVEL");
const CURSOR_OFFSET: Option<isize> = option_env_isize!("CURSOR_OFFSET");

fn main() {
    println!("Configuration loaded:");
    if let Some(mem) = MEMORY_LIMIT {
        println!("  Memory limit: {} MB", mem);
    }
    if let Some(reqs) = MAX_REQUESTS {
        println!("  Max requests: {}", reqs);
    }
    if let Some(offset) = TIMEZONE_OFFSET {
        println!("  Timezone offset: {}", offset);
    }
}
```

### String Environment Variables

```rust
use moosicbox_env_utils::default_env;

// Get string environment variable with default at compile time
const APP_NAME: &str = default_env!("APP_NAME", "MoosicBox");
const ENVIRONMENT: &str = default_env!("ENVIRONMENT", "development");

fn main() {
    println!("Application: {} ({})", APP_NAME, ENVIRONMENT);
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

### Compile-Time Macros

**Required Value Extraction:**

- `env_usize!(name)` - Extract required usize at compile time (panics if not set)

**Default Value Extraction (unsigned):**

- `default_env_usize!(name, default)` - Extract usize with default at compile time
- `default_env_u64!(name, default)` - Extract u64 with default at compile time
- `default_env_u32!(name, default)` - Extract u32 with default at compile time
- `default_env_u16!(name, default)` - Extract u16 with default at compile time

**Optional Value Extraction (unsigned):**

- `option_env_usize!(name)` - Extract Option\<usize\> at compile time
- `option_env_u64!(name)` - Extract Option\<u64\> at compile time
- `option_env_u32!(name)` - Extract Option\<u32\> at compile time
- `option_env_u16!(name)` - Extract Option\<u16\> at compile time

**Optional Value Extraction (signed):**

- `option_env_isize!(name)` - Extract Option\<isize\> at compile time
- `option_env_i64!(name)` - Extract Option\<i64\> at compile time
- `option_env_i32!(name)` - Extract Option\<i32\> at compile time
- `option_env_i16!(name)` - Extract Option\<i16\> at compile time
- `option_env_i8!(name)` - Extract Option\<i8\> at compile time

**String Values:**

- `default_env!(name, default)` - Get string environment variable with default at compile time

### Const Functions

- `parse_usize(s)` - Parse string to usize (const-compatible)
- `parse_isize(s)` - Parse string to isize with sign support (const-compatible)

## Error Handling

The library uses compile-time macros that panic on errors rather than returning Result types:

- **`env_usize!`** - Panics if the environment variable is not set at compile time
- **`option_env_*!` macros** - Panic if the environment variable is set but contains an invalid value
- **`ParseIntError`** - Error type for const parsing functions (`parse_usize`, `parse_isize`)
    - `Empty` - The input string was empty
    - `InvalidDigit` - Invalid digit encountered during parsing

Note: Because these are compile-time macros, errors are caught during compilation rather than at runtime.

## Performance

- Const functions enable compile-time evaluation for parsing
- Macro-based extraction has zero runtime cost - all values are resolved at compile time
- Custom const-compatible parsing implementation avoids standard library dependencies
