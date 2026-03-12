# switchy_uuid

Switchable UUID generation supporting both production and simulation modes.

## Overview

This package provides UUID v4 generation with configurable behavior:

- **Production mode**: Cryptographically secure random UUIDs via the standard `uuid` crate
- **Simulation mode**: Deterministic, seeded UUIDs for reproducible testing

## Features

- Switchable UUID generation based on feature flags
- Environment-controlled seeding for deterministic UUIDs
- Compatible with standard `uuid::Uuid` type
- Zero-cost abstraction when using only one mode

## Installation

```toml
[dependencies]
switchy_uuid = "0.1.0"
```

## Usage

```rust
use switchy_uuid::{new_v4, new_v4_string};

// Generate UUID
let id = new_v4();

// Generate UUID as string
let token = new_v4_string();
```

### Core `Uuid` API

`new_v4()` returns `switchy_uuid::Uuid`, which provides parsing and conversion helpers:

```rust
use switchy_uuid::Uuid;

let id = Uuid::new_v4();
let parsed = Uuid::parse_str(&id.to_string()).unwrap();
assert_eq!(id, parsed);
```

### Simulation Mode

When the `simulator` feature is enabled (default), UUIDs are generated deterministically using a seeded random number generator. Control the seed via:

```bash
SIMULATOR_UUID_SEED=12345  # Default seed if not specified
```

## Feature Flags

- `uuid` (default): Enable standard UUID generation using `uuid::Uuid::new_v4()`
- `simulator` (default): Enable deterministic UUID generation with seeded RNG
- `serde`: Enable serde serialization/deserialization support for the `Uuid` type
- `fail-on-warnings`: Treat compiler warnings as errors

**Note**: When both `uuid` and `simulator` are enabled, `simulator` takes precedence.
