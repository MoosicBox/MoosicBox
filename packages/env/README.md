# switchy_env

Deterministic environment variable access for testing and simulation.

## Features

- **Production**: Uses real environment variables via `std::env`
- **Simulation**: Uses configurable environment with deterministic defaults
- **Type Safety**: Parse environment variables to specific types
- **Testing**: Set/remove variables for testing scenarios

## Usage

```rust
use switchy_env::{var, var_or, var_parse, var_parse_or};

// Get environment variable
let database_url = var("DATABASE_URL")?;

// Get with default
let port = var_or("PORT", "8080");

// Parse to specific type
let timeout: u64 = var_parse("TIMEOUT")?;

// Parse with default
let max_connections: usize = var_parse_or("MAX_CONNECTIONS", 100);
```

## Simulator Features

In simulator mode, you can control environment variables:

```rust
use switchy_env::simulator::{set_var, remove_var, reset};

// Set variable for testing
set_var("TEST_VAR", "test_value");

// Remove variable
remove_var("TEST_VAR");

// Reset to defaults
reset();
```

## Features

- `std` (default): Enable real environment variable access
- `simulator` (default): Enable deterministic simulation mode
- `serde`: Enable serialization support
- `fail-on-warnings`: Treat warnings as errors
