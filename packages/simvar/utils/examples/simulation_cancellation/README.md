# Simulation Cancellation Example

Demonstrates how to use `simvar_utils` for managing simulation cancellation with thread-local and global cancellation tokens.

## Summary

This example shows how to run async simulations with graceful cancellation support, including thread-local cancellation, global cancellation, manual cancellation checks, and resetting cancellation state for multiple simulation runs.

## What This Example Demonstrates

- Running async tasks with `run_until_simulation_cancelled()`
- Thread-local cancellation using `cancel_simulation()`
- Global cancellation with `cancel_global_simulation()`
- Manual cancellation checks using `is_simulator_cancelled()` and `is_global_simulator_cancelled()`
- Resetting cancellation tokens with `reset_simulator_cancellation_token()` and `reset_global_simulator_cancellation_token()`
- Worker thread ID tracking with `worker_thread_id()`
- Graceful shutdown patterns in simulation scenarios

## Prerequisites

- Basic understanding of async/await in Rust
- Familiarity with cancellation patterns
- Understanding of thread-local vs. global state

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/simvar/utils/examples/simulation_cancellation/Cargo.toml
```

Or from the example directory:

```bash
cd packages/simvar/utils/examples/simulation_cancellation
cargo run
```

## Expected Output

The example will run four demonstrations:

```
Simulation Cancellation Examples
=================================

Worker Thread ID: 1

=== Example 1: Thread-Local Cancellation ===

[Thread 1] Task 1: Starting work (duration: 1000ms)
[Thread 2] Cancelling thread-local simulation
[Thread 1] Task 1: Detected cancellation at checkpoint 2
✗ Simulation was cancelled (as expected)

=== Example 2: Global Cancellation ===

[Thread 3] Task 2: Starting work (duration: 1500ms)
[Thread 4] Task 3: Starting work (duration: 1500ms)
[Thread 5] Triggering global cancellation
[Thread 3] Task 2: Detected cancellation at checkpoint 4
[Thread 4] Task 3: Detected cancellation at checkpoint 4
Task 1 result: None
Task 2 result: None

Global cancellation complete

=== Example 3: Manual Cancellation Checks ===

[Thread 1] Detected cancellation after 3 iterations

=== Example 4: Reset and Multiple Runs ===

--- Run 1 ---
[Thread 1] Task 11: Starting work (duration: 200ms)
[Thread 1] Task 11: Work completed successfully
Run 1 completed: 1100
--- Run 2 ---
[Thread 1] Task 12: Starting work (duration: 200ms)
[Thread 1] Task 12: Work completed successfully
Run 2 completed: 1200
--- Run 3 ---
[Thread 1] Task 13: Starting work (duration: 200ms)
[Thread 1] Task 13: Work completed successfully
Run 3 completed: 1300

=================================
All examples completed!
```

## Code Walkthrough

### Example 1: Thread-Local Cancellation

Demonstrates basic cancellation using `run_until_simulation_cancelled()`:

```rust
// Reset cancellation state for clean start
reset_simulator_cancellation_token();

// Spawn a task that will cancel after 250ms
spawn(async {
    sleep(Duration::from_millis(250)).await;
    cancel_simulation();
});

// Run simulation until cancelled
let result = run_until_simulation_cancelled(async {
    simulate_work(1, 1000).await
}).await;
```

The `run_until_simulation_cancelled()` function races the provided future against cancellation tokens, returning `None` if cancelled or `Some(output)` if completed.

### Example 2: Global Cancellation

Shows how global cancellation affects multiple concurrent tasks:

```rust
reset_global_simulator_cancellation_token();

// Spawn multiple tasks
let task1 = spawn(async {
    run_until_simulation_cancelled(async {
        simulate_work(2, 1500).await
    }).await
});

let task2 = spawn(async {
    run_until_simulation_cancelled(async {
        simulate_work(3, 1500).await
    }).await
});

// Trigger global cancellation
cancel_global_simulation();
```

Global cancellation triggers the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN`, which affects all threads and all tasks using `run_until_simulation_cancelled()`.

### Example 3: Manual Cancellation Checks

Demonstrates manual cancellation checking for fine-grained control:

```rust
loop {
    // Check thread-local cancellation
    if is_simulator_cancelled() {
        println!("Detected thread-local cancellation");
        break;
    }

    // Check global cancellation
    if is_global_simulator_cancelled() {
        println!("Detected global cancellation");
        break;
    }

    // Do work...
    sleep(Duration::from_millis(50)).await;
}
```

This pattern is useful when integrating with existing code or when you need explicit control over cancellation points.

### Example 4: Reset and Multiple Runs

Shows how to reset cancellation state for sequential simulation runs:

```rust
for run in 1..=3 {
    // Critical: Reset state before each run
    reset_simulator_cancellation_token();
    reset_global_simulator_cancellation_token();

    let result = run_until_simulation_cancelled(async {
        simulate_work(10 + run, 200).await
    }).await;
}
```

Resetting cancellation tokens is essential when running multiple simulations to ensure each starts with clean state.

## Key Concepts

### Thread-Local vs. Global Cancellation

- **Thread-Local**: Affects only the current thread's simulation. Use `cancel_simulation()` and `is_simulator_cancelled()`.
- **Global**: Affects all threads and simulations across the entire process. Use `cancel_global_simulation()` and `is_global_simulator_cancelled()`.

### Cancellation Token Lifecycle

1. **Creation**: Tokens are automatically created on first use
2. **Triggering**: Call `cancel_simulation()` or `cancel_global_simulation()`
3. **Checking**: Use `is_simulator_cancelled()` or `is_global_simulator_cancelled()`
4. **Resetting**: Call `reset_simulator_cancellation_token()` or `reset_global_simulator_cancellation_token()` to prepare for new runs

### Worker Thread IDs

Each thread gets a unique, monotonically increasing ID via `worker_thread_id()`. This is useful for:

- Debugging multi-threaded simulations
- Logging and tracing
- Thread-specific behavior

### Graceful Shutdown

The `run_until_simulation_cancelled()` function enables graceful shutdown by racing your future against cancellation tokens. When cancellation is triggered:

1. The future is dropped
2. `None` is returned to indicate cancellation
3. Resources are cleaned up automatically

## Testing the Example

The example is self-contained and runs automatically. To verify behavior:

1. **Observe cancellation timing**: Tasks should be interrupted mid-execution
2. **Check thread IDs**: Multiple spawned tasks will show different thread IDs
3. **Verify reset behavior**: Example 4 should complete all three runs successfully
4. **Monitor output**: Each example should print expected cancellation messages

You can modify timing values in the code to experiment with different cancellation scenarios:

```rust
// In example_1_thread_local_cancellation()
sleep(Duration::from_millis(250)).await; // Try different values
```

## Troubleshooting

### Tasks completing before cancellation

If tasks finish before being cancelled, the cancellation task may be delayed. Try:

- Increasing simulation duration: `simulate_work(1, 2000).await`
- Decreasing cancellation delay: `sleep(Duration::from_millis(100)).await`

### Cancellation not detected

Ensure you're checking cancellation status in your work loop:

```rust
if is_simulator_cancelled() {
    return; // Exit early
}
```

### State leaking between runs

Always reset cancellation tokens before starting a new simulation:

```rust
reset_simulator_cancellation_token();
reset_global_simulator_cancellation_token();
```

## Related Examples

This is currently the only example for `simvar_utils`. For related concepts, see:

- `packages/async/examples/cancel/` - General async cancellation patterns
- Simulation framework examples in other packages
