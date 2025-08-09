# switchy_uuid

Deterministic UUID generation for testing and simulation.

## Features

- **Production**: Uses cryptographically secure random UUIDs
- **Simulation**: Uses seeded deterministic UUIDs for reproducible testing
- **Environment Control**: Set `SIMULATOR_UUID_SEED` to control deterministic generation

## Usage

```rust
use switchy_uuid::{new_v4, new_v4_string};

// Generate UUID
let id = new_v4();

// Generate UUID as string
let token = new_v4_string();
```

## Features

- `uuid` (default): Enable real UUID generation
- `simulator` (default): Enable deterministic UUID generation
- `fail-on-warnings`: Treat warnings as errors
