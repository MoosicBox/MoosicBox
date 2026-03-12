# switchy_env

Deterministic environment variable access for testing and simulation.

## Features

- **Standard backend**: `switchy_env::standard::*` reads real environment variables via `std::env`
- **Simulator backend**: `switchy_env::*` uses a configurable environment with deterministic defaults when `simulator` is enabled (default)
- **Type Safety**: Parse environment variables to specific types
- **Testing**: Set/remove variables for testing scenarios

## Usage

With default features, `switchy_env::*` resolves to the simulator backend.

```rust
use switchy_env::{var, var_or, var_parse, var_parse_or, var_parse_opt, var_exists, vars};

// Get environment variable
let database_url = var("DATABASE_URL")?;

// Get with default
let port = var_or("PORT", "8080");

// Parse to specific type
let timeout: u64 = var_parse("TIMEOUT")?;

// Parse with default
let max_connections: usize = var_parse_or("MAX_CONNECTIONS", 100);

// Parse optional variable (None if not set, Some(T) if parseable, Err if unparseable)
let debug_level: Option<u32> = var_parse_opt("DEBUG_LEVEL")?;

// Check if variable exists
if var_exists("FEATURE_FLAG") {
    // ...
}

// Get all environment variables
let all_vars = vars();
```

To force real environment access when both default features are enabled, use the `standard` module:

```rust
use switchy_env::standard::var;

let home = var("HOME")?;
```

## Simulator Features

In simulator mode, you can control environment variables:

```rust
use switchy_env::simulator::{set_var, remove_var, clear, reset};

// Set variable for testing
set_var("TEST_VAR", "test_value");

// Remove variable
remove_var("TEST_VAR");

// Clear all variables
clear();

// Reset to defaults
reset();
```

## Cargo Features

- `std` (default): Enable real environment variable access
- `simulator` (default): Enable deterministic simulation mode
- `fail-on-warnings`: Treat warnings as errors

## Core Types

- `EnvProvider`: Trait for custom environment providers with methods for fetching, parsing, and enumerating variables
- `EnvError`: Error enum returned by fallible operations (`NotFound`, `InvalidValue`, `ParseError`)
- `Result<T>`: Convenience alias for `std::result::Result<T, EnvError>`
