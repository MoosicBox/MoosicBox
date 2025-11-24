# Async Simulated Example

A demonstration of concurrent task spawning and execution using the switchy async runtime with simulation capabilities.

## Overview

This example showcases how to spawn multiple concurrent tasks using the switchy async runtime. It demonstrates task spawning, nested futures, random delays, and concurrent execution patterns with deterministic simulation support.

## What it demonstrates

- **Task spawning** - Creating multiple concurrent tasks with `task::spawn`
- **Nested futures** - Spawning tasks from within other tasks
- **Random delays** - Using random number generation with async sleep
- **Concurrent execution** - Multiple tasks running simultaneously
- **Runtime management** - Proper runtime lifecycle and cleanup
- **Simulation support** - Deterministic execution with seeded randomness

## Code walkthrough

The example:

1. **Initializes the runtime** and sets up logging
2. **Spawns a main coordinator task** that creates multiple worker tasks
3. **Creates 5 concurrent tasks** with random sleep durations
4. **Each task spawns a nested task** with additional random delays
5. **Demonstrates blocking operations** alongside concurrent tasks
6. **Waits for all tasks to complete** before shutting down

## Key concepts

### Task Spawning

```rust
task::spawn(async move {
    println!("Spawned Fn #{:02}: Start {}", i, time());
    time::sleep(Duration::from_millis(1000 * random)).await;
    // ... more work
});
```

Creating concurrent tasks that run independently.

### Nested Task Spawning

```rust
task::spawn(async move {
    time::sleep(Duration::from_millis(1000 * random2)).await;
    println!("Spawned Fn #{:02}: Inner {}", i, time());
});
```

Spawning additional tasks from within existing tasks.

### Random Delays

```rust
let random = rng().gen_range(1..10);
time::sleep(Duration::from_millis(1000 * random)).await;
```

Using seeded random number generation for predictable simulation.

### Runtime Coordination

```rust
runtime.block_on(async {
    time::sleep(Duration::from_millis(11000)).await;
});
runtime.wait()?;
```

Coordinating between blocking operations and spawned tasks.

## Running the example

```bash
cargo run --package async_simulated
```

### With simulation features

```bash
cargo run --package async_simulated --features simulator
```

## Expected output

The output will show concurrent execution with timestamps:

```
block on
Begin Asynchronous Execution (seed=12345)
Spawned Fn #00: Start 1634664688
Spawned Fn #01: Start 1634664688
Spawned Fn #02: Start 1634664688
Spawned Fn #03: Start 1634664688
Spawned Fn #04: Start 1634664688
Spawned Fn #01: Ended 1634664690
Spawned Fn #01: Inner 1634664691
Spawned Fn #04: Ended 1634664694
Spawned Fn #04: Inner 1634664695
Spawned Fn #00: Ended 1634664697
Spawned Fn #02: Ended 1634664697
Spawned Fn #03: Ended 1634664697
Spawned Fn #00: Inner 1634664698
Spawned Fn #03: Inner 1634664698
Spawned Fn #02: Inner 1634664702
Blocking Function Polled To Completion
End of Asynchronous Execution
```

**Note**: The exact order and timing will vary between runs due to randomness, but with simulation features enabled, execution becomes deterministic.

## Use cases

This pattern is useful for:

- **Concurrent processing** - Running multiple independent operations
- **Background tasks** - Spawning work that doesn't block main execution
- **Fan-out patterns** - Distributing work across multiple tasks
- **Testing async code** - Deterministic simulation for reliable tests
- **Load simulation** - Simulating concurrent user requests or operations

## Features

### Simulation Support

When built with the `simulator` feature:

- **Deterministic execution** - Same seed produces same results
- **Controlled timing** - Predictable scheduling for testing
- **Reproducible behavior** - Consistent results across runs

### Random Number Generation

- **Seeded RNG** - Uses `switchy_random` with simulation support
- **Deterministic in simulation** - Predictable random values for testing
- **Variable delays** - Creates realistic timing variations

## Dependencies

- `switchy_async` - Async runtime with task spawning (features: "time", "tokio")
- `switchy_random` - Random number generation with simulation support (features: "simulator")
- `switchy_time` - Time utilities with simulation support (features: "simulator")
- `pretty_env_logger` - Logging setup

## Comparison with other examples

| Example                       | Focus                | Key Feature               |
| ----------------------------- | -------------------- | ------------------------- |
| [Cancel](../cancel/README.md) | Graceful shutdown    | Cancellation tokens       |
| **Simulated**                 | Concurrent execution | Task spawning and nesting |

## Related

- [`switchy_async`](../../README.md) - Main async runtime package
- [`switchy_random`](../../../random/README.md) - Random number generation
- [Cancel Example](../cancel/README.md) - Cancellation and shutdown patterns
