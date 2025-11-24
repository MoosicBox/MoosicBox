# Basic Usage Example

This example demonstrates the fundamental usage patterns of the `moosicbox_async_service` framework, showing how to create an async service with sequential command processing, lifecycle management, and graceful shutdown.

## Summary

A simple task processing service that demonstrates command handling, state management, lifecycle hooks, and the different ways to send commands to an async service.

## What This Example Demonstrates

- Creating an async service using the `async_service_sequential!` macro
- Defining custom command types and service context
- Implementing the `Processor` trait for command handling
- Using lifecycle hooks (`on_start` and `on_shutdown`)
- Sending commands asynchronously without waiting
- Sending commands and waiting for completion
- Managing shared state with `Arc<RwLock<T>>`
- Gracefully shutting down the service

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with the concept of actor/service patterns
- Understanding of shared state with locks (`RwLock`)

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/async_service/examples/basic_usage/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/async_service/examples/basic_usage
cargo run
```

## Expected Output

When you run this example, you should see output similar to:

```
=== MoosicBox Async Service Example ===

🚀 Task processor service starting...
✅ Service started

📋 Processing task: 'Download file'
   Total tasks processed: 1
📋 Processing task: 'Parse data'
   Total tasks processed: 2
📊 Status: Processing task: Parse data
   Tasks processed: 2

⏳ Sending command and waiting for completion...
📋 Processing task: 'Generate report'
   Total tasks processed: 3
✅ Command completed

➕ Counter incremented to: 4
➕ Counter incremented to: 5
📊 Status: Processing task: Generate report
   Tasks processed: 5

🛑 Shutting down service...
🛑 Task processor service shutting down...
   Final task count: 5

=== Example completed successfully ===
```

## Code Walkthrough

### 1. Define Commands

```rust
#[derive(Debug)]
pub enum TaskCommand {
    ProcessTask { name: String },
    GetStatus,
    IncrementCounter,
}
```

Commands represent the operations your service can perform. Each variant can carry data needed for that operation.

### 2. Define Service Context

```rust
pub struct TaskContext {
    pub processed_count: u32,
    pub status: String,
}
```

The context holds the service's state, which is wrapped in `Arc<RwLock<T>>` for safe concurrent access.

### 3. Generate the Service

```rust
async_service_sequential!(TaskCommand, TaskContext);
```

This macro generates:

- `Service` struct for managing the service lifecycle
- `Handle` struct for sending commands
- `Commander` trait with methods like `send_command_async()`
- `Error` enum for error handling
- `Processor` trait to implement

### 4. Implement Command Processing

```rust
#[async_trait]
impl Processor for Service {
    type Error = Error;

    async fn process_command(
        ctx: Arc<sync::RwLock<TaskContext>>,
        command: TaskCommand,
    ) -> Result<(), Self::Error> {
        match command {
            TaskCommand::ProcessTask { name } => {
                let mut context = ctx.write().await;
                context.processed_count += 1;
                // ... process the task
            }
            // ... handle other commands
        }
        Ok(())
    }
}
```

The `process_command` method is called for each command. Use read locks for queries and write locks for mutations.

### 5. Lifecycle Hooks

```rust
async fn on_start(&mut self) -> Result<(), Self::Error> {
    // Initialize resources, connect to databases, etc.
    Ok(())
}

async fn on_shutdown(ctx: Arc<sync::RwLock<TaskContext>>) -> Result<(), Self::Error> {
    // Clean up resources, close connections, save state, etc.
    Ok(())
}
```

These hooks are called when the service starts and stops, allowing you to manage resources properly.

### 6. Start and Use the Service

```rust
// Create the service with initial context
let service = Service::new(context).with_name("TaskProcessor");
let handle = service.handle();
let join_handle = service.start();

// Send commands
handle.send_command_async(command).await?;

// Send and wait for completion
handle.send_command_and_wait_async(command).await?;

// Shutdown
handle.shutdown()?;
join_handle.await??;
```

The handle can be cloned and shared across tasks, making it easy to send commands from anywhere in your application.

## Key Concepts

### Sequential vs Concurrent Processing

This example uses `async_service_sequential!`, which processes commands one at a time in order. For concurrent processing where multiple commands can run in parallel, use `async_service!` instead.

### Command Sending Patterns

- **`send_command_async()`**: Sends the command and returns immediately. Use for fire-and-forget operations.
- **`send_command_and_wait_async()`**: Sends the command and waits until it completes. Use when you need confirmation that the command was processed.

### State Management

The service context is wrapped in `Arc<RwLock<T>>`:

- `Arc` allows multiple tasks to share ownership
- `RwLock` provides safe concurrent access (multiple readers or one writer)
- Use `.read().await` for read-only access
- Use `.write().await` for mutable access

### Graceful Shutdown

Always call `shutdown()` and `await` the join handle to ensure:

- All pending commands are processed
- The `on_shutdown` hook runs
- Resources are properly cleaned up

## Testing the Example

Try modifying the example to:

1. Add a new command type (e.g., `ResetCounter`)
2. Add more fields to the context (e.g., `last_task_time: Option<Instant>`)
3. Implement error handling in `process_command` (return an error for certain conditions)
4. Clone the handle and send commands from multiple concurrent tasks

## Troubleshooting

### Service Not Processing Commands

Ensure you're calling `.await?` after sending commands. If you send commands but don't await the futures, they won't execute.

### Deadlocks When Accessing Context

Avoid holding locks across `.await` points:

```rust
// Bad - lock held across await
let mut ctx = ctx.write().await;
some_async_function().await; // Lock still held!
ctx.value = 42;

// Good - lock released before await
{
    let mut ctx = ctx.write().await;
    ctx.value = 42;
} // Lock released here
some_async_function().await;
```

### Commands Not Completing Before Shutdown

Use `send_command_and_wait_async()` for critical commands, or add a delay before shutdown to allow async commands to complete.

## Related Examples

This is currently the only example for `moosicbox_async_service`. Future examples might cover:

- Concurrent command processing with `async_service!`
- Error handling and recovery
- Integration with other MoosicBox services
- Advanced patterns like command batching or prioritization
