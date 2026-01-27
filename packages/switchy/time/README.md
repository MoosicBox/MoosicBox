# switchy_time

Time utilities with support for both standard system time and simulated time for deterministic testing.

## Overview

This crate provides a unified interface for getting the current time, with the ability to switch between real system time and simulated time. When the `simulator` feature is enabled (default), time can be controlled programmatically for deterministic testing.

## Features

- **`std`** (default) - Enables standard library time functions
- **`simulator`** (default) - Enables time simulation capabilities for testing
- **`chrono`** - Adds support for `chrono` `DateTime` types

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_time = "0.1.4"
```

## Usage

### Basic Time Functions

```rust
use switchy_time::{now, instant_now};
use std::time::SystemTime;

// Get current time (simulated when simulator feature is enabled)
let current_time: SystemTime = now();

// Get current monotonic instant
let instant = instant_now();
```

### With Chrono Support

```rust
use switchy_time::{datetime_utc_now, datetime_local_now};

// Requires `chrono` feature
let utc_time = datetime_utc_now();
let local_time = datetime_local_now();
```

### Time Simulation (Testing)

When the `simulator` feature is enabled, time is controlled via a step-based system:

```rust
use switchy_time::simulator::{
    now, next_step, set_step, reset_step,
    epoch_offset, step_multiplier, with_real_time
};

// Reset simulation state
reset_step();

// Get simulated time
let time1 = now();

// Advance time by one step
next_step();
let time2 = now();  // time2 > time1

// Set specific step
set_step(100);

// Temporarily use real system time
let real_time = with_real_time(|| now());
```

Time simulation is calculated as: `epoch_offset + (step * step_multiplier)` milliseconds since Unix epoch.

### Environment Variables

The simulator supports environment variables to control time values:

- `SIMULATOR_EPOCH_OFFSET` - Base Unix timestamp in milliseconds
- `SIMULATOR_STEP_MULTIPLIER` - Milliseconds per step

## License

See the [LICENSE](../../../LICENSE) file for details.
