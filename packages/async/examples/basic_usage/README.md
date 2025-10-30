# Basic Usage Example

This example demonstrates the fundamental usage patterns of the switchy async runtime. It provides a comprehensive walkthrough of creating a runtime, executing async operations, spawning tasks, and managing the runtime lifecycle.

## What This Example Demonstrates

- **Runtime Creation**: Using the `Builder` API to create a runtime
- **block_on Usage**: Running async operations synchronously
- **Task Spawning**: Creating background tasks with `task::spawn`
- **Task Coordination**: Awaiting task results and managing dependencies
- **Multiple Concurrent Tasks**: Running several tasks in parallel
- **Runtime Lifecycle**: Proper initialization and cleanup

## Prerequisites

- Rust toolchain (see root README)
- Basic understanding of async/await in Rust
- Familiarity with futures and tasks

## Running the Example

```bash
# From repository root
cargo run --manifest-path packages/async/examples/basic_usage/Cargo.toml

# Or from example directory
cd packages/async/examples/basic_usage
cargo run
```

## Expected Output

```
=== Switchy Async Basic Usage Example ===

1. Creating runtime with Builder...
   Runtime created successfully

2. Running simple async operation with block_on...
   Inside async block
   After sleep
   Result: 42

3. Spawning background task...
   Background task started
   Main async block doing other work
   Background task completing
   Background task result: task result

4. Spawning multiple concurrent tasks...
   Task 1 starting
   Task 2 starting
   Task 3 starting
   Task 1 completing
   Task 2 completing
   Task 3 completing
   Waiting for all tasks to complete...
   Task 1 result: 10
   Task 2 result: 20
   Task 3 result: 30

5. Demonstrating task coordination...
   Producer: generating data
   Consumer: waiting for data
   Producer: data ready
   Consumer: received 5 items
   Consumer: sum = 15
   Final result: 15

6. Cleaning up runtime...
   Runtime shut down successfully

=== Example completed successfully ===
```

## Code Walkthrough

### 1. Creating the Runtime

```rust
use switchy_async::{Builder, GenericRuntime};

let runtime = Builder::new().build()?;
```

The `Builder` provides a fluent API for configuring the runtime. With default settings, it creates a multi-threaded Tokio runtime or simulator runtime depending on feature flags.

**Configuration options:**

```rust
let runtime = Builder::new()
    .max_blocking_threads(Some(4))  // Set max blocking threads (requires rt-multi-thread feature)
    .build()?;
```

### 2. Running Async Code with block_on

```rust
let result = runtime.block_on(async {
    time::sleep(Duration::from_millis(100)).await;
    42
});
```

`block_on()` runs an async operation to completion, blocking the current thread. This is the primary entry point for executing async code from synchronous contexts.

**When to use:**

- Main function entry point
- Test functions
- Bridging sync and async code
- Running one-off async operations

### 3. Spawning Background Tasks

```rust
let handle = task::spawn(async {
    println!("Background task started");
    time::sleep(Duration::from_millis(200)).await;
    "task result"
});

// Do other work...

let result = handle.await?;
```

`task::spawn()` creates a new task that runs concurrently in the background. It returns a `JoinHandle` that can be awaited to get the result.

**Key characteristics:**

- Tasks run independently on the runtime
- Multiple tasks execute concurrently
- Awaiting the handle blocks until the task completes
- Tasks continue even if the handle is dropped (fire-and-forget)

### 4. Multiple Concurrent Tasks

```rust
let mut handles = Vec::new();

for i in 1..=3 {
    let handle = task::spawn(async move {
        // Task work
        i * 10
    });
    handles.push(handle);
}

for handle in handles {
    let result = handle.await?;
    // Process result
}
```

This pattern demonstrates spawning multiple tasks and collecting their results. All tasks run concurrently, improving throughput for independent operations.

### 5. Task Coordination

```rust
let producer = task::spawn(async {
    // Generate data
    vec![1, 2, 3, 4, 5]
});

let consumer = task::spawn(async move {
    let data = producer.await?;
    // Process data
    data.iter().sum()
});

let result = consumer.await?;
```

Tasks can depend on other tasks by awaiting their handles. This creates a task dependency graph where consumers wait for producers.

### 6. Runtime Cleanup

```rust
runtime.wait()?;
```

Waits for all spawned tasks to complete before dropping the runtime. This ensures:

- No tasks are interrupted mid-execution
- All background work finishes
- Resources are released cleanly

## Key Concepts

### Runtime Abstraction

Switchy async provides a unified interface across different runtime backends:

- **Tokio backend**: Production runtime with full async I/O support
- **Simulator backend**: Deterministic runtime for testing

Same code works with both backends by changing feature flags.

### block_on vs task::spawn

| Operation       | Blocks Thread | Returns Immediately  | Use Case                     |
| --------------- | ------------- | -------------------- | ---------------------------- |
| `block_on()`    | Yes           | No                   | Entry point, sync/async gap  |
| `task::spawn()` | No            | Yes (returns handle) | Background work, concurrency |

### Task Lifecycle

1. **Spawned**: Task created with `task::spawn()`
2. **Running**: Task executes on the runtime
3. **Completed**: Task finishes, result available via handle
4. **Awaited**: Handle consumed to retrieve result

### GenericRuntime Trait

The `GenericRuntime` trait provides runtime-agnostic operations:

```rust
pub trait GenericRuntime {
    fn block_on<F: Future>(&self, future: F) -> F::Output;
    fn wait(self) -> Result<(), Error>;
}
```

This allows writing code that works with any runtime implementation.

## Testing the Example

### Modifying Task Count

Change the number of concurrent tasks:

```rust
for i in 1..=10 {  // Change from 3 to 10
    let handle = task::spawn(async move {
        // ...
    });
    handles.push(handle);
}
```

### Adding Sleep Durations

Adjust sleep times to observe timing:

```rust
time::sleep(Duration::from_secs(1)).await;  // 1 second instead of milliseconds
```

### Fire-and-Forget Tasks

Spawn tasks without storing handles:

```rust
task::spawn(async {
    println!("Fire-and-forget task");
    time::sleep(Duration::from_millis(100)).await;
});
// Task continues running even though handle is dropped
```

## Troubleshooting

### Runtime Creation Fails

**Problem**: `Builder::new().build()` returns an error

**Solution**: Ensure the required features are enabled. Default features include `tokio` and `rt-multi-thread`. Check your `Cargo.toml` dependencies.

### Tasks Not Completing

**Problem**: Program exits before tasks finish

**Solution**: Always call `runtime.wait()` before the program exits to ensure all spawned tasks complete.

### Deadlocks

**Problem**: Program hangs indefinitely

**Solution**: Check for circular dependencies between tasks. If task A awaits task B, and B awaits A, they will deadlock.

## Related Examples

- **[Cancel Example](../cancel/README.md)**: Demonstrates cancellation tokens for graceful shutdown
- **[Simulated Example](../simulated/README.md)**: Shows complex task spawning patterns and simulation features

## Use Cases

These basic patterns enable:

- **Async applications**: Building fully async programs from the ground up
- **Background processing**: Running work in parallel without blocking main logic
- **Testing infrastructure**: Creating test harnesses with async runtime
- **Migration from sync**: Incrementally adding async to existing codebases
- **Cross-runtime compatibility**: Writing code that works with multiple async runtimes
