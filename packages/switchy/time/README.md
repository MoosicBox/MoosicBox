# switchy_time

Time utilities with support for both standard system time and simulated time for testing.

## Overview

This crate provides a unified interface for getting the current time, with the ability to switch between real system time and simulated time for testing purposes. When the `simulator` feature is enabled, time can be controlled programmatically for deterministic testing.

## Features

- **`std`** (default) - Enables standard library time functions
- **`simulator`** (default) - Enables time simulation capabilities for testing
- **`chrono`** - Adds support for `chrono` `DateTime` types

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_time = { workspace = true }
```

## Usage

### Basic Time Functions

```rust
use switchy_time::{now, instant_now};
use std::time::SystemTime;

// Get the current system time
let current_time: SystemTime = now();

// Get the current monotonic instant
let instant = instant_now();
```

### With Chrono Support

When the `chrono` feature is enabled:

```rust
use switchy_time::{datetime_utc_now, datetime_local_now};

// Get the current UTC date and time
let utc_now = datetime_utc_now();

// Get the current local date and time
let local_now = datetime_local_now();
```

### Time Simulation (with `simulator` feature)

The simulator module enables deterministic testing of time-dependent code. Time simulation is based on three components:

- **Epoch offset** - The base Unix timestamp in milliseconds
- **Step counter** - The current simulation step
- **Step multiplier** - How many milliseconds of simulated time pass per step

Simulated time is calculated as: `epoch_offset + (step * step_multiplier)`

```rust
use switchy_time::simulator::{
    now, instant_now, set_step, next_step, reset_step,
    epoch_offset, step_multiplier, with_real_time
};

// Reset the step counter to zero
reset_step();

// Advance time by one step
let step = next_step();

// Set a specific step
let step = set_step(100);

// Get the current epoch offset and step multiplier
let offset = epoch_offset();
let multiplier = step_multiplier();

// Temporarily use real system time instead of simulated time
let real_time = with_real_time(|| {
    now()
});
```

#### Environment Variables

The simulator can be configured via environment variables:

- `SIMULATOR_EPOCH_OFFSET` - Override the epoch offset (milliseconds since Unix epoch)
- `SIMULATOR_STEP_MULTIPLIER` - Override the step multiplier (milliseconds per step)

## License

This project is licensed under the MPL-2.0 License.
