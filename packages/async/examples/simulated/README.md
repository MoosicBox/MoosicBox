# Task Spawning and Concurrent Execution Example

This example demonstrates concurrent task spawning and execution using the switchy async runtime. It showcases how to create multiple independent tasks, spawn nested tasks, and coordinate concurrent operations with deterministic simulation support for testing.

## What This Example Demonstrates

- **Task Spawning**: Creating multiple concurrent tasks with `task::spawn`
- **Nested Tasks**: Spawning additional tasks from within existing tasks
- **Concurrent Execution**: Multiple tasks running independently
- **Random Delays**: Using seeded random number generation with async sleep
- **Runtime Coordination**: Managing `runtime.spawn()`, `runtime.block_on()`, and `runtime.wait()`
- **Simulation Support**: Deterministic execution with reproducible behavior for testing

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust and task spawning
- Basic familiarity with concurrent programming concepts

## Running the Example

```bash
# From repository root
cargo run --manifest-path packages/async/examples/simulated/Cargo.toml

# Or from example directory
cd packages/async/examples/simulated
cargo run

# With simulator features for deterministic execution
cargo run --features simulator
```

## Expected Output

The output shows concurrent task execution with timestamps:

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

**Note**: Without the `simulator` feature, execution order will vary between runs due to randomness. With simulation enabled, behavior becomes deterministic and reproducible.

## Code Walkthrough

### 1. Runtime Initialization

```rust
pretty_env_logger::init();
let runtime = Runtime::new();
```

Creates a new runtime instance and initializes logging for debugging.

### 2. Spawning the Coordinator Task

```rust
runtime.spawn(async {
    let seed = initial_seed();
    println!("Begin Asynchronous Execution (seed={seed})");
    // ... spawn worker tasks
});
```

Uses `runtime.spawn()` to create a non-blocking task that coordinates other tasks. This task continues running in the background.

### 3. Creating Multiple Concurrent Tasks

```rust
for i in 0..5 {
    let random = rng().gen_range(1..10);
    let random2 = rng().gen_range(1..10);

    task::spawn(async move {
        println!("Spawned Fn #{:02}: Start {}", i, time());
        time::sleep(Duration::from_millis(1000 * random)).await;
        // ...
    });
}
```

Spawns 5 independent tasks, each with randomized sleep duration. Tasks run concurrently without blocking each other.

### 4. Nested Task Spawning

```rust
task::spawn(async move {
    time::sleep(Duration::from_millis(1000 * random2)).await;
    println!("Spawned Fn #{:02}: Inner {}", i, time());
});
println!("Spawned Fn #{:02}: Ended {}", i, time());
```

Each worker task spawns an additional nested task before completing, demonstrating multi-level concurrency.

### 5. Blocking Operations

```rust
runtime.block_on(async {
    println!("block on");
    time::sleep(Duration::from_millis(11000)).await;
    println!("Blocking Function Polled To Completion");
});
```

Uses `block_on()` to execute an async operation synchronously. This blocks the main thread but doesn't prevent spawned tasks from running.

### 6. Runtime Cleanup

```rust
runtime.wait()?;
println!("End of Asynchronous Execution");
```

Waits for all spawned tasks to complete before exiting. This ensures no tasks are dropped prematurely.

## Key Concepts

### Task Spawning vs Runtime Methods

- **`task::spawn()`**: Creates independent tasks that run in the background
- **`runtime.spawn()`**: Similar to `task::spawn()` but returns no handle
- **`runtime.block_on()`**: Runs a future to completion, blocking the current thread

### Concurrent Execution Model

Tasks spawned with `task::spawn()` run concurrently:

- Each task has its own execution context
- Tasks yield control at `.await` points
- Runtime schedules tasks cooperatively
- No guarantee of execution order

### Seeded Randomness for Testing

```rust
let seed = initial_seed();
let random = rng().gen_range(1..10);
```

Using seeded RNG allows:

- **Reproducibility**: Same seed produces same behavior
- **Deterministic testing**: Tests pass consistently
- **Debugging**: Can replay exact execution scenarios

### Time Utilities

```rust
let time = || {
    now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
};
```

Captures timestamps to observe task execution order and timing.

## Testing the Example

### Observing Concurrent Execution

Run the example and observe:

1. All 5 tasks start simultaneously (same timestamp)
2. Tasks complete in different orders based on random delays
3. Nested tasks execute after their parent tasks sleep
4. The blocking operation completes independently

### Testing Deterministic Behavior

```bash
# Run with simulator feature twice
cargo run --features simulator
cargo run --features simulator
```

Both runs should produce identical output, demonstrating reproducibility.

### Modifying Task Count

Edit the example to spawn more or fewer tasks:

```rust
for i in 0..10 {  // Change from 5 to 10
    // ...
}
```

## Troubleshooting

### Tasks Not Running

**Problem**: Only "block on" output appears, no task output

**Solution**: Ensure `runtime.wait()` is called to allow spawned tasks to complete. Without it, tasks may be dropped when the runtime exits.

### Non-Deterministic Output

**Problem**: Output order changes between runs

**Solution**: This is expected without the `simulator` feature. Use `--features simulator` for deterministic behavior.

### Long Execution Time

**Problem**: Example takes many seconds to complete

**Solution**: Random delays range from 1-10 seconds. The blocking operation also sleeps for 11 seconds. This is intentional to demonstrate concurrent timing.

## Related Examples

- **[Cancel Example](../cancel/README.md)**: Demonstrates cancellation tokens and graceful shutdown
- **[Basic Usage Example](../basic_usage/README.md)**: Shows fundamental runtime creation and simple async operations

## Use Cases

This concurrent task pattern is useful for:

- **Parallel data processing**: Processing multiple items concurrently
- **Background workers**: Running maintenance tasks alongside main logic
- **Fan-out operations**: Distributing work across multiple tasks
- **Load testing**: Simulating multiple concurrent clients or requests
- **Testing async code**: Deterministic simulation for reliable unit tests
- **Event processing**: Handling multiple events concurrently
