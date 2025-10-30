# Basic Async Service Example

A comprehensive example demonstrating the core features of the `moosicbox_async_service` framework for building async services with command processing and lifecycle management.

## Summary

This example shows how to create a fully functional async service that processes commands sequentially, manages state through a context object, and implements lifecycle hooks for startup and shutdown operations.

## What This Example Demonstrates

- Creating a service with the `async_service_sequential!` macro
- Defining command enums for different operations
- Managing mutable state through a service context
- Implementing the `Processor` trait for command handling
- Sending commands asynchronously with `send_command_async()`
- Waiting for command completion with `send_command_and_wait_async()`
- Using lifecycle hooks (`on_start`, `on_shutdown`)
- Proper service shutdown and cleanup
- Sequential command processing (commands executed in order)

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with channels and message passing
- Understanding of `Arc` and `RwLock` for shared state

## Running the Example

```bash
cargo run --manifest-path packages/async_service/examples/basic_service/Cargo.toml
```

## Expected Output

```
=== MoosicBox Async Service Example ===

[Main] Creating service...
[Main] Starting service...

[Lifecycle] Service starting up...
[Main] Sending async commands...
  [Service] Processing task 1: First task
  [Service] Task 1 completed
  [Service] Processing task 2: Second task
  [Service] Task 2 completed

[Main] Sending command and waiting for completion...
  [Service] Starting heavy computation with value 7
  [Service] Computation complete: 7^2 = 49
[Main] Heavy computation completed

[Main] Requesting status...
  [Service] Status: Processed task 2 | Tasks: 2 | Result: 49

[Main] Sending batch of tasks...
  [Service] Processing task 3: Batch task 3
  [Service] Task 3 completed
  [Service] Processing task 4: Batch task 4
  [Service] Task 4 completed
  [Service] Processing task 5: Batch task 5
  [Service] Task 5 completed

[Main] Final status check...
  [Service] Status: Processed task 5 | Tasks: 5 | Result: 49

[Main] Shutting down service...
[Lifecycle] Service shutting down. Final stats: 5 tasks processed

[Main] Service shutdown complete

=== Example Complete ===
```

## Code Walkthrough

### 1. Defining Commands

Commands represent the operations your service can perform:

```rust
#[derive(Debug)]
pub enum TaskCommand {
    ProcessTask { id: u32, data: String },
    GetStatus,
    HeavyComputation { value: u32 },
}
```

### 2. Creating the Service Context

The context holds the mutable state of your service:

```rust
pub struct TaskContext {
    pub tasks_processed: u32,
    pub status: String,
    pub computation_result: u32,
}
```

### 3. Generating the Service Infrastructure

The `async_service_sequential!` macro generates all the boilerplate:

```rust
async_service_sequential!(TaskCommand, TaskContext);
```

This creates:

- `Service` struct for managing the service lifecycle
- `Handle` struct for sending commands
- `Processor` trait for implementing command handling
- `Error` enum for error types
- `Commander` trait with command sending methods

### 4. Implementing Command Processing

The `Processor` trait defines how commands are handled:

```rust
#[async_trait]
impl Processor for Service {
    type Error = Error;

    async fn process_command(
        ctx: Arc<sync::RwLock<TaskContext>>,
        command: TaskCommand,
    ) -> Result<(), Self::Error> {
        match command {
            TaskCommand::ProcessTask { id, data } => {
                // Access and modify context
                let mut context = ctx.write().await;
                context.tasks_processed += 1;
            }
            // ... other commands
        }
        Ok(())
    }
}
```

### 5. Lifecycle Hooks

Implement optional hooks for startup and shutdown:

```rust
async fn on_start(&mut self) -> Result<(), Self::Error> {
    println!("Service starting up...");
    Ok(())
}

async fn on_shutdown(ctx: Arc<sync::RwLock<TaskContext>>) -> Result<(), Self::Error> {
    println!("Service shutting down...");
    Ok(())
}
```

### 6. Starting the Service

Create the service, get a handle, and start it:

```rust
let context = TaskContext { /* ... */ };
let service = Service::new(context).with_name("TaskProcessor");
let handle = service.handle();
let join_handle = service.start();
```

### 7. Sending Commands

Use the handle to send commands:

```rust
// Fire and forget
handle.send_command_async(TaskCommand::ProcessTask {
    id: 1,
    data: "First task".to_string(),
}).await?;

// Send and wait for completion
handle.send_command_and_wait_async(TaskCommand::HeavyComputation {
    value: 7
}).await?;
```

### 8. Shutting Down

Gracefully shutdown the service:

```rust
handle.shutdown()?;
join_handle.await??;
```

## Key Concepts

### Sequential vs Concurrent Processing

This example uses **sequential processing** (`async_service_sequential!`):

- Commands are processed one at a time in the order received
- Each command completes before the next one begins
- Useful when order matters or when modifying shared state

For **concurrent processing**, use `async_service!` instead:

- Each command spawns its own task
- Multiple commands can run in parallel
- Better for I/O-bound operations or independent commands

### Command Flow

1. Commands are sent through channels to the service
2. The service receives them from its async loop
3. Commands are processed by `process_command()`
4. For `send_command_and_wait_async()`, completion is signaled back to the caller

### State Management

- Context is wrapped in `Arc<RwLock<T>>` for safe concurrent access
- Use `.read().await` for read-only access
- Use `.write().await` for mutable access
- Multiple readers can access simultaneously, but writes are exclusive

### Error Handling

The macro generates an `Error` enum with common error types:

- `Error::Join` - Task join errors
- `Error::Send` - Command sending errors
- `Error::IO` - I/O errors

You can also define custom error types by using the three-argument macro form:

```rust
async_service_sequential!(MyCommand, MyContext, MyCustomError);
```

## Testing the Example

Run the example and observe:

1. **Startup**: The service starts and prints its lifecycle hook message
2. **Command Processing**: Each command is processed in order with logged output
3. **Synchronous Waiting**: The heavy computation command blocks the caller until complete
4. **Status Queries**: The service can report its current state
5. **Batch Processing**: Multiple commands are queued and processed sequentially
6. **Shutdown**: The service cleanly shuts down with final statistics

## Troubleshooting

### Service Doesn't Receive Commands

- Ensure you call `service.start()` before sending commands
- Make sure you're using `.await` on async send operations
- Check that the service hasn't already shut down

### Commands Not Processed in Order

- Verify you're using `async_service_sequential!` not `async_service!`
- Sequential processing guarantees order within a single service instance

### Deadlocks or Hangs

- Avoid holding write locks across await points when possible
- Don't call service commands from within `process_command()` (creates circular dependency)
- Ensure you call `shutdown()` and `await` the join handle

## Related Examples

This is currently the only example for `moosicbox_async_service`. Additional examples demonstrating concurrent processing, error handling patterns, and integration with other MoosicBox components may be added in the future.
