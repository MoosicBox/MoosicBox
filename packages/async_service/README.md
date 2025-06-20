# MoosicBox Async Service

Asynchronous service management framework for the MoosicBox ecosystem, providing basic service lifecycle management, command processing, and task execution utilities for building concurrent applications.

## Features

- **Service Framework**: Basic async service definition and management
- **Command Processing**: Channel-based command processing with async handlers
- **Lifecycle Management**: Service start, stop, and shutdown handling
- **Task Spawning**: Utilities for spawning async tasks
- **Cancellation Support**: Built-in cancellation token support
- **Error Handling**: Service-specific error types and handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_async_service = "0.1.1"
```

## Usage

### Creating a Service

```rust
use moosicbox_async_service::*;

// Define your command types
#[derive(Debug)]
pub enum MyCommand {
    ProcessData { data: String },
    GetStatus,
    Shutdown,
}

// Define your service context
pub struct MyContext {
    pub processed_count: u32,
    pub status: String,
}

// Use the async_service_body macro to generate the service
async_service_body!(MyCommand, MyContext, true); // true = sequential processing

// Implement the Processor trait
impl Processor for Service {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn process_command(
        ctx: Arc<tokio::sync::RwLock<MyContext>>,
        command: MyCommand,
    ) -> Result<(), Self::Error> {
        match command {
            MyCommand::ProcessData { data } => {
                println!("Processing: {}", data);
                let mut context = ctx.write().await;
                context.processed_count += 1;
                context.status = format!("Processed: {}", data);
            }
            MyCommand::GetStatus => {
                let context = ctx.read().await;
                println!("Status: {} (count: {})", context.status, context.processed_count);
            }
            MyCommand::Shutdown => {
                println!("Shutting down service");
            }
        }
        Ok(())
    }

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        println!("Service starting");
        Ok(())
    }

    async fn on_shutdown(
        ctx: Arc<tokio::sync::RwLock<MyContext>>,
    ) -> Result<(), Self::Error> {
        println!("Service shutting down");
        Ok(())
    }
}
```

### Running the Service

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create service context
    let context = MyContext {
        processed_count: 0,
        status: "Ready".to_string(),
    };

    // Create and start the service
    let service = Service::new(context)
        .with_name("MyDataProcessor");

    let handle = service.handle();
    let join_handle = service.start();

    // Send commands to the service
    handle.send_command_async(MyCommand::ProcessData {
        data: "Hello World".to_string()
    }).await?;

    handle.send_command_async(MyCommand::GetStatus).await?;

    // Send command and wait for completion
    handle.send_command_and_wait_async(MyCommand::ProcessData {
        data: "Important Data".to_string()
    }).await?;

    // Shutdown the service
    handle.shutdown()?;

    // Wait for service to complete
    join_handle.await??;

    Ok(())
}
```

### Command Handling Options

```rust
// Sequential processing (commands processed one at a time)
async_service_body!(MyCommand, MyContext, true);

// Concurrent processing (commands processed in parallel)
async_service_body!(MyCommand, MyContext, false);
```

### Error Handling

```rust
use moosicbox_async_service::CommanderError;

match handle.send_command_async(command).await {
    Ok(()) => println!("Command sent successfully"),
    Err(CommanderError::Send) => eprintln!("Failed to send command"),
    Err(CommanderError::Recv(e)) => eprintln!("Receive error: {}", e),
}
```

## Core Components

### Service
The main service struct that manages command processing and lifecycle.

### Handle
A cloneable handle for sending commands to the service from other tasks.

### Commander Trait
Provides methods for sending commands:
- `send_command()`: Send without waiting
- `send_command_async()`: Send asynchronously without waiting
- `send_command_and_wait_async()`: Send and wait for completion

### Processor Trait
Define how your service processes commands and handles lifecycle events.

## Dependencies

The library re-exports commonly used async utilities:
- `tokio`: Async runtime and utilities
- `async_trait`: Async trait support
- `flume`: Fast async channels
- `futures`: Additional async utilities
- `moosicbox_task`: Task spawning utilities

This framework provides a foundation for building robust async services in the MoosicBox ecosystem.
