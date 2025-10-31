# Async Simulated Example

A demonstration of concurrent task spawning and execution using the switchy async runtime with simulation capabilities.

## Overview

This example showcases how to spawn multiple concurrent tasks using the switchy async runtime. It demonstrates task spawning, nested futures, random delays, and concurrent execution patterns with deterministic simulation support.

## What This Example Demonstrates

- **Task spawning** - Creating multiple concurrent tasks with `task::spawn`
- **Nested futures** - Spawning tasks from within other tasks
- **Random delays** - Using random number generation with async sleep
- **Concurrent execution** - Multiple tasks running simultaneously
- **Runtime management** - Proper runtime lifecycle and cleanup
- **Simulation support** - Deterministic execution with seeded randomness

## Prerequisites

- Understanding of async task spawning and futures
- Familiarity with concurrent programming concepts
- Basic knowledge of random number generation (helpful but not required)
- No additional setup required - example runs standalone

## Running the Example

```bash
cargo run --manifest-path packages/async/examples/simulated/Cargo.toml
```

### With Simulation Features

```bash
cargo run --manifest-path packages/async/examples/simulated/Cargo.toml --features simulator
```

## Expected Output

The output will show concurrent execution with timestamps:

```
Begin Asynchronous Execution (seed=12345)
block on
Blocking Function Polled To Completion
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
End of Asynchronous Execution
```

**Note**: The exact order and timing will vary between runs due to randomness, but with simulation features enabled, execution becomes deterministic.

## Code Walkthrough

The example:

1. **Initializes the runtime** and sets up logging
2. **Spawns a main coordinator task** that creates multiple worker tasks
3. **Creates 5 concurrent tasks** with random sleep durations
4. **Each task spawns a nested task** with additional random delays
5. **Demonstrates blocking operations** alongside concurrent tasks
6. **Waits for all tasks to complete** before shutting down

## Key Concepts

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

## Testing the Example

Try experimenting with the code:

1. **Run with different backends**:

    - Default (Tokio): Non-deterministic execution, real-time delays
    - With `--features simulator`: Deterministic execution with controlled time

2. **Observe concurrency**:

    - Watch the order of "Start" messages (all appear together)
    - Note that "Ended" messages appear at different times based on random delays
    - See how nested tasks complete after their parent tasks

3. **Modify task count**:

    - Change the loop from `0..5` to `0..10` to spawn more tasks
    - Observe how the runtime handles increased concurrency

4. **Adjust delays**:
    - Modify the `gen_range(1..10)` values to change delay ranges
    - See how this affects the interleaving of task completion

## Use Cases

This pattern is useful for:

- **Concurrent processing** - Running multiple independent operations
- **Background tasks** - Spawning work that doesn't block main execution
- **Fan-out patterns** - Distributing work across multiple tasks
- **Testing async code** - Deterministic simulation for reliable tests
- **Load simulation** - Simulating concurrent user requests or operations

## Troubleshooting

### Tasks complete in unexpected order

- This is normal! Concurrent tasks don't have a guaranteed execution order
- Use the `simulator` feature for deterministic ordering in tests
- Check that you're not making assumptions about task completion sequence

### "Blocking Function Polled To Completion" appears before spawned tasks

- This is expected behavior - `block_on` is polled to completion first
- Spawned tasks continue running after `block_on` returns
- The `runtime.wait()` call ensures all spawned tasks complete

### Different output each run

- Random number generation causes timing variations
- Use `--features simulator` for reproducible execution
- The seed is printed at the start for debugging purposes

## Advanced Features

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

## Related Examples

- [Basic Usage Example](../basic_usage/README.md) - Runtime fundamentals and simple task spawning
- [Cancel Example](../cancel/README.md) - Graceful shutdown with cancellation tokens
