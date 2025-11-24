# Basic Usage Example

A comprehensive introduction to the switchy_async runtime demonstrating fundamental async programming concepts.

## What This Example Demonstrates

- Creating an async runtime with the `Builder` pattern
- Running async code with `block_on`
- Calling async functions and awaiting results
- Spawning concurrent background tasks with `task::spawn`
- Getting results from spawned tasks with `JoinHandle`
- Proper runtime lifecycle management with `wait()`

## Prerequisites

- Basic understanding of Rust async/await syntax
- Familiarity with futures and tasks concepts
- No additional setup required - example runs standalone

## Running the Example

```bash
cargo run --manifest-path packages/async/examples/basic_usage/Cargo.toml
```

## Expected Output

```
=== Switchy Async Basic Usage Example ===

1. Creating async runtime...
   Runtime created successfully

2. Running simple async function with block_on...
   Received: Hello, World!
   Result: Hello, World!

3. Spawning concurrent background tasks...
Background worker 1 starting
Background worker 2 starting
Background worker 3 starting
Background worker 1 completed
Background worker 2 completed
Background worker 3 completed
   All background tasks completed

4. Getting results from spawned tasks...
   Computation result: 84

5. Shutting down runtime...
   Runtime shut down cleanly

=== Example completed successfully ===
```

**Note**: The order of "Background worker X starting/completed" messages may vary due to concurrent execution.

## Code Walkthrough

### 1. Creating the Runtime

```rust
let runtime = Builder::new().build()?;
```

The `Builder` provides a fluent API for configuring the runtime. Call `build()` to create a `Runtime` instance that implements the `GenericRuntime` trait.

### 2. Running Async Code with block_on

```rust
let result = runtime.block_on(async {
    let message = greet("World").await;
    message
});
```

`block_on` executes an async block on the runtime and waits for it to complete, returning the final value. This is your entry point into async code from synchronous contexts.

### 3. Spawning Concurrent Tasks

```rust
let handle = task::spawn(async {
    // Task code here
});
```

`task::spawn` creates a new concurrent task that runs independently. It returns a `JoinHandle` that can be awaited to get the task's result.

### 4. Getting Task Results

```rust
match computation.await {
    Ok(result) => println!("Result: {result}"),
    Err(e) => println!("Task failed: {e}"),
}
```

Awaiting a `JoinHandle` yields a `Result<T, JoinError>` containing either the task's return value or an error if the task panicked.

### 5. Clean Shutdown

```rust
runtime.wait()?;
```

The `wait()` method ensures all spawned tasks complete before the runtime shuts down. Always call this before program exit to avoid dropping pending work.

## Key Concepts

### Runtime Abstraction

The `GenericRuntime` trait provides a common interface across different async runtime backends (Tokio, simulator). Your code works with either backend without changes.

### Task Concurrency

Tasks spawned with `task::spawn` run concurrently, not sequentially. Multiple tasks make progress simultaneously, with the runtime scheduling their execution.

### Async/Await

The `async` keyword creates a future, and `await` suspends execution until that future completes. This allows other tasks to run while waiting.

### Error Handling

Both `build()` and `wait()` return `Result<_, Error>` types. Always handle these with `?` or explicit error handling to catch runtime initialization or shutdown failures.

## Testing the Example

Try modifying the example to:

1. **Change task count**: Spawn more or fewer background workers
2. **Adjust delays**: Modify `Duration::from_millis()` values to see timing effects
3. **Add computations**: Create tasks that perform calculations and return results
4. **Experiment with errors**: Make a task panic and see how `JoinError` is handled

## Troubleshooting

### "Runtime failed to build" error

- Ensure the `switchy_async` default features are enabled
- Check that either the `tokio` or `simulator` backend feature is active

### Tasks not completing

- Make sure you call `runtime.wait()` before the program exits
- Verify you're awaiting all `JoinHandle` values you care about

### Compilation errors about Send

- Ensure spawned tasks only capture `Send` types
- Use `spawn_local` for `!Send` futures (requires entering the runtime context)

## Related Examples

- [Simulated Example](../simulated/README.md) - Advanced task spawning with nested futures
- [Cancel Example](../cancel/README.md) - Graceful shutdown with cancellation tokens
