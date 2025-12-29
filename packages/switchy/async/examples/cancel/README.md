# Async Cancel Example

Demonstrates cancellation token usage with the Switchy async runtime.

## What it does

This example shows how to use `CancellationToken` to gracefully shutdown async operations. It sets up a Ctrl+C handler that cancels a long-running sleep operation.

## The code

The example:

1. Creates a global `CancellationToken`
2. Sets up a Ctrl+C signal handler that calls `TOKEN.cancel()`
3. Runs `time::sleep(Duration::MAX)` inside `TOKEN.run_until_cancelled()`
4. When Ctrl+C is pressed, the sleep is cancelled and the program exits cleanly

## Key parts

### Global cancellation token

```rust
static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
```

### Signal handler

```rust
fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}
```

### Cancellable operation

```rust
runtime.block_on(TOKEN.run_until_cancelled(async move {
    println!("Blocking Function. Press ctrl+c to exit");
    time::sleep(Duration::MAX).await;
    println!("Blocking Function Polled To Completion");
}));
```

## Running it

```bash
cargo run --package async_cancel
```

Press Ctrl+C to trigger cancellation.

## Expected output

```
Blocking Function. Press ctrl+c to exit
^Cctrl+c received. shutting runtime down...
After block_on
Runtime shut down cleanly
```

## Dependencies

- `switchy_async` - Async runtime with cancellation support
- `ctrlc` - Ctrl+C signal handling
- `pretty_env_logger` - Logging setup

## Related

- [`switchy_async`](../../README.md) - Main async runtime package
- [Simulated Example](../simulated/README.md) - Task spawning example
