# switchy_uuid

Switchable UUID generation supporting both production and simulation modes.

## Overview

This package provides UUID v4 generation with configurable behavior:

- **Production mode**: UUID v4 generation from OS entropy; no seeded fallback
- **Simulation mode**: Deterministic, seeded UUIDs for reproducible testing

## Features

- Switchable UUID generation based on feature flags
- Per-run deterministic UUIDs in simulation, derived from the run's seed
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

When the `simulator` feature is enabled (default), UUIDs are drawn from the current
simulation run's `switchy_uuid` RNG stream (`switchy_simulation`). The sequence is derived
from the run's seed, so the same seed reproduces the same UUIDs, each run restarts the
sequence, and parallel runs (each with its own simulation context) are isolated from one
another. Choose the seed with `SIMULATOR_SEED`:

```bash
SIMULATOR_SEED=12345
```

`SIMULATOR_UUID_SEED` is no longer read.

## Feature Flags

- `native`: Enable UUID v4 generation from `switchy_random::os_fill_bytes`. In a custom-getrandom build explicit native generation fails closed (the existing infallible API panics).
- `uuid-compat`: Keep the const `inner`/`from_inner`/`into_inner` and external conversion traits. This is the only normal dependency on the registry `uuid` crate.
- `uuid` (default): Enable `native` and `uuid-compat` for existing consumers.
- `simulator` (default): Enable deterministic UUID generation from the run's seed
- `serde`: Enable serde serialization/deserialization support for the `Uuid` type
- `fail-on-warnings`: Treat compiler warnings as errors

**Migration:** Disable default features and select `native`, `simulator`, or neither (value-only), plus `serde` if needed. These builds store network-order `[u8; 16]` without `uuid`. If a no-default consumer uses an external conversion, add `uuid-compat` explicitly. Cargo features unify: every dependency edge must omit `uuid` and `uuid-compat` to remove the registry dependency. Parsing and formatting are owned in both modes; parse errors remain `ParseError` but diagnostic detail is not a wire contract.

When native generation and simulation are both enabled, `simulator` takes precedence for `Uuid::new_v4()`.
