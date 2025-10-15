# Switchy Time

A simple time abstraction library providing unified time access with support for both standard system time and simulated time for testing.

## Features

- **Time Abstraction**: Unified `now()` and `instant_now()` functions that work with different time backends
- **Standard Time**: Use system time for production scenarios
- **Simulated Time**: Controllable time simulation for testing and development
- **Step Control**: Manually advance simulated time in discrete steps
- **Epoch Offset**: Configurable time offset for simulation scenarios
- **Thread Local State**: Per-thread time simulation state management
- **Chrono Integration**: Optional `chrono` support with `datetime_local_now()` and `datetime_utc_now()` functions

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_time = "0.1.4"

# Choose your backend
switchy_time = { version = "0.1.4", features = ["std"] }
# or for testing
switchy_time = { version = "0.1.4", features = ["simulator"] }
# or with chrono support
switchy_time = { version = "0.1.4", features = ["simulator", "chrono"] }
```

## Usage

### Basic Time Access

```rust
use switchy_time::{now, instant_now};
use std::time::{SystemTime, Instant};

fn main() {
    // Get current system time
    let current_time: SystemTime = now();
    println!("Current time: {:?}", current_time);

    // Get current instant (monotonic time)
    let current_instant: Instant = instant_now();
    println!("Current instant: {:?}", current_instant);

    // Time behaves like SystemTime::now() in standard mode
    let duration_since_epoch = current_time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();

    println!("Seconds since epoch: {}", duration_since_epoch.as_secs());
}
```

### Simulated Time (Testing)

```rust
#[cfg(feature = "simulator")]
use switchy_time::simulator::{now, instant_now, reset_step, next_step, set_step, current_step};

#[cfg(feature = "simulator")]
fn test_with_simulated_time() {
    // Reset to initial state
    reset_step();

    let time1 = now();
    let instant1 = instant_now();
    println!("Step 0 time: {:?}, instant: {:?}", time1, instant1);

    // Advance time by one step
    next_step();
    let time2 = now();
    let instant2 = instant_now();
    println!("Step 1 time: {:?}, instant: {:?}", time2, instant2);

    // Jump to specific step
    set_step(100);
    let time3 = now();
    println!("Step 100 time: {:?}", time3);

    // Check current step
    println!("Current step: {}", current_step());
}
```

### Time Simulation Configuration

```rust
#[cfg(feature = "simulator")]
use switchy_time::simulator::{
    reset_epoch_offset, epoch_offset,
    reset_step_multiplier, step_multiplier
};

#[cfg(feature = "simulator")]
fn configure_simulation() {
    // Reset epoch offset (randomized base time)
    reset_epoch_offset();
    println!("Epoch offset: {}", epoch_offset());

    // Reset step multiplier (time advancement per step)
    reset_step_multiplier();
    println!("Step multiplier: {}", step_multiplier());

    // Environment variables can control these values:
    // SIMULATOR_EPOCH_OFFSET - sets the epoch offset (milliseconds)
    // SIMULATOR_STEP_MULTIPLIER - sets the step multiplier (milliseconds)
}
```

### Real Time in Simulation Mode

```rust
#[cfg(feature = "simulator")]
use switchy_time::simulator::{with_real_time, now, instant_now};

#[cfg(feature = "simulator")]
fn use_real_time_temporarily() {
    // In simulator mode, get simulated time
    let simulated_time = now();
    let simulated_instant = instant_now();
    println!("Simulated time: {:?}", simulated_time);

    // Temporarily use real system time
    let real_time = with_real_time(|| {
        (now(), instant_now()) // Returns actual SystemTime::now() and Instant::now()
    });
    println!("Real time: {:?}", real_time);

    // Back to simulated time
    let simulated_again = now();
    println!("Simulated time again: {:?}", simulated_again);
}
```

### Chrono Integration

```rust
#[cfg(all(feature = "simulator", feature = "chrono"))]
use switchy_time::simulator::{datetime_local_now, datetime_utc_now, reset_step, set_step};

#[cfg(all(feature = "simulator", feature = "chrono"))]
fn use_chrono_datetime() {
    reset_step();

    // Get current time as chrono DateTime
    let local_time = datetime_local_now();
    let utc_time = datetime_utc_now();

    println!("Local time: {}", local_time);
    println!("UTC time: {}", utc_time);

    // Advance time
    set_step(1000);
    let later_time = datetime_utc_now();
    println!("Later UTC time: {}", later_time);
}
```

### Testing Time-Dependent Code

```rust
#[cfg(feature = "simulator")]
use switchy_time::{now, simulator::{reset_step, next_step, set_step}};
use std::time::Duration;

#[cfg(feature = "simulator")]
struct TimestampedEvent {
    timestamp: std::time::SystemTime,
    data: String,
}

#[cfg(feature = "simulator")]
fn test_time_dependent_logic() {
    reset_step();

    let mut events = Vec::new();

    // Create events at different time steps
    for i in 0..5 {
        set_step(i * 1000); // Each step is 1000 multiplier units apart

        events.push(TimestampedEvent {
            timestamp: now(),
            data: format!("Event {}", i),
        });
    }

    // Verify event ordering
    for (i, event) in events.iter().enumerate() {
        println!("Event {}: {} at {:?}", i, event.data, event.timestamp);

        if i > 0 {
            let duration = event.timestamp
                .duration_since(events[i-1].timestamp)
                .unwrap();
            println!("  Time since previous: {:?}", duration);
        }
    }
}
```

### Environment Configuration

The simulator can be configured via environment variables:

```bash
# Set a specific epoch offset (milliseconds since Unix epoch)
export SIMULATOR_EPOCH_OFFSET=1640995200000

# Set step multiplier (milliseconds per step)
export SIMULATOR_STEP_MULTIPLIER=1000

# Run your application
cargo run --features simulator
```

## API Reference

### Universal Functions

- `now()` - Returns `SystemTime` from appropriate backend
- `instant_now()` - Returns `Instant` from appropriate backend

### Chrono Functions (with `chrono` feature)

- `datetime_local_now()` - Returns `chrono::DateTime<chrono::Local>` from appropriate backend
- `datetime_utc_now()` - Returns `chrono::DateTime<chrono::Utc>` from appropriate backend

### Standard Backend (`std` feature)

- Uses `std::time::SystemTime::now()` and `std::time::Instant::now()` directly

### Simulator Backend (`simulator` feature)

- `now()` - Returns simulated time based on current step
- `instant_now()` - Returns simulated instant based on current step
- `reset_step()` - Reset step counter to 0
- `next_step()` - Advance to next step and return new step number
- `set_step(step)` - Set specific step number
- `current_step()` - Get current step number
- `reset_epoch_offset()` - Generate new random epoch offset
- `epoch_offset()` - Get current epoch offset
- `reset_step_multiplier()` - Generate new random step multiplier
- `step_multiplier()` - Get current step multiplier
- `with_real_time(f)` - Execute function with real system time
- `datetime_local_now()` - Returns simulated `chrono::DateTime<chrono::Local>` (requires `chrono` feature)
- `datetime_utc_now()` - Returns simulated `chrono::DateTime<chrono::Utc>` (requires `chrono` feature)

## Time Calculation

In simulator mode, time is calculated as:

```
time = UNIX_EPOCH + Duration::from_millis(epoch_offset + (step * step_multiplier))
```

- **epoch_offset**: Base time offset (randomized or from environment)
- **step**: Current step counter (controlled by your code)
- **step_multiplier**: Milliseconds per step (randomized or from environment)

## Cargo Features

- `std` - Enable standard system time backend (default)
- `simulator` - Enable time simulation backend (default)
- `chrono` - Enable chrono DateTime support

## Use Cases

- **Production**: Use `std` feature for normal time operations
- **Testing**: Use `simulator` feature for deterministic time testing
- **Development**: Use `simulator` feature to test time-dependent logic
- **Benchmarking**: Control time advancement for consistent measurements

## Thread Safety

Each thread maintains its own simulation state (step, epoch offset, step multiplier) using thread-local storage.
