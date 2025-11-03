# Cancellation Token Example

Demonstrates graceful shutdown using `CancellationToken` with signal handling.

## What This Example Demonstrates

- Using `CancellationToken` for graceful shutdown
- Integrating signal handlers (Ctrl+C) with async cancellation
- Running futures with automatic cancellation via `run_until_cancelled()`
- Proper cleanup and runtime shutdown after cancellation

## Prerequisites

- Basic understanding of async/await and runtime lifecycle
- Familiarity with signal handling concepts
- No additional setup required - example runs standalone

## Running the Example

```bash
cargo run --manifest-path packages/async/examples/cancel/Cargo.toml
```

Press Ctrl+C to trigger cancellation and observe the graceful shutdown.

## Expected Output

```
Blocking Function. Press ctrl+c to exit
^Cctrl+c received. shutting runtime down...
After block_on
Runtime shut down cleanly
```

## Code Walkthrough

The example:

1. Creates a global `CancellationToken`
2. Sets up a Ctrl+C signal handler that calls `TOKEN.cancel()`
3. Runs `time::sleep(Duration::MAX)` inside `TOKEN.run_until_cancelled()`
4. When Ctrl+C is pressed, the sleep is cancelled and the program exits cleanly

## Key Concepts

### Global Cancellation Token

```rust
static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
```

The `CancellationToken` is created once globally and shared across signal handlers and async tasks.

### Signal Handler Integration

```rust
fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}
```

The `ctrlc` crate provides cross-platform signal handling. When Ctrl+C is pressed, the handler calls `TOKEN.cancel()` to trigger shutdown.

### Cancellable Operation

```rust
runtime.block_on(TOKEN.run_until_cancelled(async move {
    println!("Blocking Function. Press ctrl+c to exit");
    time::sleep(Duration::MAX).await;
    println!("Blocking Function Polled To Completion");
}));
```

The `run_until_cancelled()` method wraps a future and automatically cancels it when the token is triggered. The future completes immediately upon cancellation, even if it was sleeping or waiting.

### Graceful Shutdown Pattern

1. Long-running operation starts within `run_until_cancelled()`
2. User presses Ctrl+C
3. Signal handler calls `TOKEN.cancel()`
4. The future exits immediately without completing
5. Runtime continues and calls `wait()` for cleanup
6. Program exits cleanly

## Testing the Example

1. **Run the example** - It will display "Blocking Function. Press ctrl+c to exit"
2. **Press Ctrl+C** - Observe the cancellation message and clean shutdown
3. **Verify output** - Confirm "Runtime shut down cleanly" appears

Try modifying the example:

- Change `Duration::MAX` to a shorter duration and see cancellation vs. natural completion
- Add multiple tasks wrapped in `run_until_cancelled()` to see concurrent cancellation
- Create child tokens with `TOKEN.child_token()` for hierarchical cancellation

## Troubleshooting

### Ctrl+C doesn't work or panics

- Ensure you're running in a terminal that supports signal handling
- Check that the `ctrlc` crate is compatible with your platform
- Verify the signal handler was registered successfully

### "Blocking Function Polled To Completion" appears

- This message only appears if the sleep completes naturally before cancellation
- With `Duration::MAX`, you should never see this unless cancellation fails
- If you see it, check that the signal handler is actually calling `TOKEN.cancel()`

### Runtime doesn't shut down cleanly

- Always call `runtime.wait()` before program exit
- Ensure all tasks respond to cancellation (don't ignore it)
- Check for deadlocks or tasks that never yield

## Related Examples

- [Basic Usage Example](../basic_usage/README.md) - Runtime fundamentals and task spawning
- [Simulated Example](../simulated/README.md) - Concurrent task execution patterns
