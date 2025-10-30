# Cancellation Token Example

This example demonstrates how to use `CancellationToken` to gracefully shutdown async operations in the switchy async runtime. It shows how to respond to system signals (Ctrl+C) and cancel long-running operations cleanly.

## What This Example Demonstrates

- **CancellationToken Usage**: Creating and using cancellation tokens for graceful shutdown
- **Signal Handling**: Integrating Ctrl+C signal handling with async runtime
- **Cancellable Operations**: Wrapping async operations with `run_until_cancelled()`
- **Runtime Lifecycle**: Proper runtime startup, cancellation, and cleanup
- **Graceful Shutdown**: Ensuring clean program termination

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust basics
- Familiarity with signal handling concepts

## Running the Example

```bash
# From repository root
cargo run --manifest-path packages/async/examples/cancel/Cargo.toml

# Or from example directory
cd packages/async/examples/cancel
cargo run
```

**Interaction**: Press Ctrl+C to trigger cancellation and observe graceful shutdown.

## Expected Output

```
Blocking Function. Press ctrl+c to exit
^Cctrl+c received. shutting runtime down...
After block_on
Runtime shut down cleanly
```

The program will wait indefinitely until you press Ctrl+C, at which point it cancels the operation and exits cleanly.

## Code Walkthrough

### 1. Global Cancellation Token

```rust
static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
```

Creates a global cancellation token that can be accessed from signal handlers. Using `LazyLock` ensures thread-safe lazy initialization.

### 2. Signal Handler Setup

```rust
fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}

fn main() -> Result<(), Error> {
    ctrlc::set_handler(ctrl_c).unwrap();
    // ...
}
```

Registers a Ctrl+C handler that calls `cancel()` on the global token. This allows system signals to trigger graceful cancellation.

### 3. Runtime Creation

```rust
let runtime = Runtime::new();
```

Creates a new async runtime instance using default configuration.

### 4. Cancellable Operation

```rust
runtime.block_on(TOKEN.run_until_cancelled(async move {
    println!("Blocking Function. Press ctrl+c to exit");
    time::sleep(Duration::MAX).await;
    println!("Blocking Function Polled To Completion");
}));
println!("After block_on");
```

Wraps a long-running operation (sleeping for maximum duration) with `run_until_cancelled()`. When the token is cancelled, the future completes immediately without waiting for the sleep.

### 5. Runtime Cleanup

```rust
runtime.wait()?;
println!("Runtime shut down cleanly");
```

Waits for the runtime to finish and releases resources.

## Key Concepts

### CancellationToken Pattern

A cancellation token provides cooperative cancellation:

- **Non-blocking**: Cancellation doesn't forcefully terminate operations
- **Cooperative**: Operations check the token and respond appropriately
- **Composable**: Multiple operations can share the same token
- **Hierarchical**: Tokens can be cloned to create child tokens

### run_until_cancelled Semantics

```rust
TOKEN.run_until_cancelled(future)
```

This combinator:

1. Runs the future normally
2. Monitors the cancellation token in parallel
3. If token is cancelled, completes immediately with `()`
4. If future completes first, returns its result

### Graceful Shutdown Pattern

This example demonstrates a common shutdown pattern:

1. **Create token** at program startup
2. **Register handler** for system signals
3. **Wrap operations** with `run_until_cancelled()`
4. **Clean up** after cancellation

## Testing the Example

### Normal Cancellation Flow

1. Run the example
2. Wait for "Press ctrl+c to exit" message
3. Press Ctrl+C
4. Observe graceful shutdown messages

### Without Cancellation

If you don't press Ctrl+C, the program will run indefinitely (sleeping for `Duration::MAX` which is effectively forever).

## Troubleshooting

### Signal Not Working

**Problem**: Ctrl+C doesn't trigger shutdown

**Solution**: Ensure terminal is in the foreground and the example is running (not hung during startup)

### Immediate Exit

**Problem**: Program exits without waiting for Ctrl+C

**Solution**: Check that `time::sleep(Duration::MAX)` isn't being optimized away or that the runtime is properly initialized

## Related Examples

- **[Simulated Example](../simulated/README.md)**: Demonstrates task spawning and concurrent execution
- **[Basic Usage Example](../basic_usage/README.md)**: Shows fundamental runtime creation and usage patterns

## Use Cases

This cancellation pattern is useful for:

- **Web servers**: Graceful shutdown on termination signals
- **Background workers**: Clean cancellation of long-running jobs
- **CLI applications**: Responding to user interrupts
- **Batch processing**: Cancelling multi-stage pipelines
- **Testing**: Controlling test execution duration
