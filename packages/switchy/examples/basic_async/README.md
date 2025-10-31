# Basic Async Runtime Example

This example demonstrates how to use switchy's async runtime abstraction layer to write code that works with multiple async backends (Tokio and simulator) by simply switching feature flags.

## Summary

Switchy provides a runtime-agnostic async interface that allows you to write async code once and run it with different backends. This example showcases basic async operations including sleep, spawning tasks, and using async macros like `join!`, `select!`, and `try_join!`.

## What This Example Demonstrates

- Basic async sleep operations with `Duration`
- Spawning concurrent tasks with `spawn`
- Using `join!` macro to wait for multiple concurrent operations
- Using `select!` macro to race multiple operations
- Using `try_join!` macro for error handling with concurrent operations
- Measuring elapsed time with `Instant`
- Switching between Tokio and simulator runtimes via feature flags

## Prerequisites

- Rust 1.70 or later
- Basic understanding of async/await in Rust
- Familiarity with async runtimes (Tokio)

## Running the Example

### With Tokio Runtime (Default)

```bash
cargo run --manifest-path packages/switchy/examples/basic_async/Cargo.toml --features async-tokio,async-macros
```

### With Simulator Runtime

The simulator runtime is useful for deterministic testing where you need precise control over time and task scheduling:

```bash
cargo run --manifest-path packages/switchy/examples/basic_async/Cargo.toml --features simulator,async-macros
```

### Without Async Macros

If you only want the basic async functionality without `join!`, `select!`, and `try_join!` macros:

```bash
cargo run --manifest-path packages/switchy/examples/basic_async/Cargo.toml --features async-tokio
```

## Expected Output

When running with the Tokio runtime and all features enabled, you should see output similar to:

```
Switchy Basic Async Example (Using Tokio Runtime)

=== Sleep Operations ===
Sleeping for 1 second...
Slept for 1.000...s

Sleeping for 500 milliseconds...
Slept for 500...ms

=== Spawning Tasks ===
Task 1 completed
Task 2 completed
Task 1 result: 42
Task 2 result: 100
Sum of results: 142

=== Join Operations ===
Join operation 3 completed
Join operation 1 completed
Join operation 2 completed
Results: first, second, third

=== Select Operations ===
Async block completed (50ms)
Async block won the race!

=== Try Join Operations ===
Try join operation 1 succeeded
Try join operation 2 succeeded
Results: 42, 100
Sum: 142

=== Example Complete ===
This example used the Tokio runtime backend.
To use the simulator backend, compile with --features simulator
```

## Code Walkthrough

### Sleep Operations

The example demonstrates basic async sleep using `switchy::unsync::time::sleep`:

```rust
use switchy::unsync::time::{sleep, Duration, Instant};

let start = Instant::now();
sleep(Duration::from_secs(1)).await;
let elapsed = start.elapsed();
```

This code works identically with both Tokio and simulator runtimes. The abstraction allows you to write the code once and switch backends via features.

### Spawning Tasks

Concurrent task execution is demonstrated using `switchy::unsync::spawn`:

```rust
use switchy::unsync::spawn;

let handle = spawn(async {
    sleep(Duration::from_millis(100)).await;
    42
});

let result = handle.await.unwrap();
```

The spawned task runs concurrently and returns a value when completed.

### Join Macro

The `join!` macro allows waiting for multiple concurrent operations to all complete:

```rust
use switchy::unsync::join;

let (result1, result2, result3) = join!(
    async { /* operation 1 */ },
    async { /* operation 2 */ },
    async { /* operation 3 */ }
);
```

All operations run concurrently, and the macro waits until all complete before returning their results as a tuple.

### Select Macro

The `select!` macro races multiple operations and completes when the first one finishes:

```rust
use switchy::unsync::select;

select! {
    _ = sleep(Duration::from_millis(100)) => {
        println!("First timer");
    }
    _ = sleep(Duration::from_millis(200)) => {
        println!("Second timer");
    }
}
```

Only one branch will execute - the one that completes first.

### Try Join Macro

For error handling with concurrent operations, `try_join!` waits for all operations but short-circuits on the first error:

```rust
use switchy::unsync::try_join;

let (result1, result2) = try_join!(
    async { Ok::<_, std::io::Error>(42) },
    async { Ok::<_, std::io::Error>(100) }
)?;
```

If any operation returns an `Err`, the entire macro returns that error immediately.

## Key Concepts

### Runtime Abstraction

Switchy provides a unified interface that abstracts over different async runtimes. This allows you to:

- Write code once that works with multiple backends
- Switch backends by changing feature flags (no code changes)
- Test with the deterministic simulator runtime
- Deploy with the production Tokio runtime

### Feature Flags

The example uses these feature flags:

- `async-tokio`: Use Tokio as the async runtime backend
- `simulator`: Use the deterministic simulator runtime for testing
- `async-macros`: Enable `join!`, `select!`, and `try_join!` macros

### Time Abstractions

The `switchy::unsync::time` module provides:

- `Duration`: Time span representation
- `Instant`: Point in time for measuring elapsed time
- `sleep`: Async delay function

All time operations work consistently across both Tokio and simulator runtimes.

## Testing the Example

### Testing Sleep Behavior

Run the example and observe the sleep durations:

1. The first sleep should take approximately 1 second
2. The second sleep should take approximately 500 milliseconds
3. The elapsed times printed should match the requested durations

### Testing Concurrent Execution

In the spawn demonstration:

1. Two tasks are spawned concurrently
2. Task 1 completes after 100ms
3. Task 2 completes after 200ms
4. Both tasks should complete, and their results should sum to 142

### Testing Join Behavior

In the join demonstration:

1. Three operations run concurrently
2. All three must complete before results are returned
3. Results are returned in the order specified in the macro, not completion order

### Testing Select Behavior

In the select demonstration:

1. Multiple operations race
2. Only the fastest operation (50ms async block) completes
3. Other operations are cancelled

## Troubleshooting

### "This example requires the async-tokio feature"

If you see this message, you forgot to enable the `async-tokio` feature. Run with:

```bash
cargo run --manifest-path packages/switchy/examples/basic_async/Cargo.toml --features async-tokio
```

### Macro-related Compilation Errors

If you encounter errors related to `join!`, `select!`, or `try_join!`, ensure you have the `async-macros` feature enabled:

```bash
cargo run --manifest-path packages/switchy/examples/basic_async/Cargo.toml --features async-tokio,async-macros
```

### Unexpected Timing

When using the Tokio runtime, actual sleep durations may vary slightly due to OS scheduling. The simulator runtime provides deterministic timing for testing.

## Related Examples

- `packages/async/examples/cancel/` - Demonstrates cancellation with switchy's async runtime
- `packages/async/examples/simulated/` - Shows how to use the simulator runtime for testing
- `packages/fs/examples/temp_dir/` - Example using switchy's filesystem abstractions

🤖 Generated with [Claude Code](https://claude.com/claude-code)
