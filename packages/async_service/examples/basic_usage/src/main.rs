#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_async_service`
//!
//! This example demonstrates how to create a simple async service with sequential
//! command processing, lifecycle hooks, and graceful shutdown.

use moosicbox_async_service::{Arc, async_service_sequential, async_trait, log, sync};

/// Commands that our service can process
#[derive(Debug)]
pub enum TaskCommand {
    /// Process a task with the given name
    ProcessTask { name: String },
    /// Get the current status of the service
    GetStatus,
    /// Increment the task counter
    IncrementCounter,
}

/// Context that holds the service state
pub struct TaskContext {
    /// Number of tasks processed
    pub processed_count: u32,
    /// Current status message
    pub status: String,
}

// Generate the async service with sequential command processing
// This creates the Service, Handle, Commander trait, Error enum, and more
async_service_sequential!(TaskCommand, TaskContext);

/// Implement the Processor trait to define how commands are handled
#[async_trait]
impl Processor for Service {
    type Error = Error;

    /// Process individual commands
    ///
    /// This method is called for each command sent to the service.
    /// Commands are processed sequentially in the order they are received.
    async fn process_command(
        ctx: Arc<sync::RwLock<TaskContext>>,
        command: TaskCommand,
    ) -> Result<(), Self::Error> {
        match command {
            TaskCommand::ProcessTask { name } => {
                // Acquire write lock to modify state
                let processed_count = {
                    let mut context = ctx.write().await;
                    context.processed_count += 1;
                    context.status = format!("Processing task: {name}");
                    context.processed_count
                };

                println!("📋 Processing task: '{name}'");
                println!("   Total tasks processed: {processed_count}");
            }
            TaskCommand::GetStatus => {
                // Acquire read lock to read state
                let (status, processed_count) = {
                    let context = ctx.read().await;
                    (context.status.clone(), context.processed_count)
                };
                println!("📊 Status: {status}");
                println!("   Tasks processed: {processed_count}");
            }
            TaskCommand::IncrementCounter => {
                // Simple state modification
                let processed_count = {
                    let mut context = ctx.write().await;
                    context.processed_count += 1;
                    context.processed_count
                };
                println!("➕ Counter incremented to: {processed_count}");
            }
        }
        Ok(())
    }

    /// Called when the service starts
    ///
    /// Use this hook to perform initialization tasks like connecting to databases,
    /// loading configuration, or setting up resources.
    async fn on_start(&mut self) -> Result<(), Self::Error> {
        println!("🚀 Task processor service starting...");
        Ok(())
    }

    /// Called when the service shuts down
    ///
    /// Use this hook to perform cleanup tasks like closing connections,
    /// flushing buffers, or saving state.
    async fn on_shutdown(ctx: Arc<sync::RwLock<TaskContext>>) -> Result<(), Self::Error> {
        let processed_count = {
            let context = ctx.read().await;
            context.processed_count
        };
        println!("🛑 Task processor service shutting down...");
        println!("   Final task count: {processed_count}");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Async Service Example ===\n");

    // Step 1: Create the service context with initial state
    let context = TaskContext {
        processed_count: 0,
        status: "Initialized".to_string(),
    };

    // Step 2: Create the service with a descriptive name
    let service = Service::new(context).with_name("TaskProcessor");

    // Step 3: Get a handle to interact with the service
    // Handles are cloneable and can be shared across tasks
    let handle = service.handle();

    // Step 4: Start the service
    // This spawns a background task that processes commands
    let join_handle = service.start();

    println!("✅ Service started\n");

    // Step 5: Send commands asynchronously without waiting
    // These commands are queued and processed in order
    handle
        .send_command_async(TaskCommand::ProcessTask {
            name: "Download file".to_string(),
        })
        .await?;

    handle
        .send_command_async(TaskCommand::ProcessTask {
            name: "Parse data".to_string(),
        })
        .await?;

    handle.send_command_async(TaskCommand::GetStatus).await?;

    // Step 6: Send a command and wait for it to complete
    // This ensures the command is processed before continuing
    println!("\n⏳ Sending command and waiting for completion...");
    handle
        .send_command_and_wait_async(TaskCommand::ProcessTask {
            name: "Generate report".to_string(),
        })
        .await?;
    println!("✅ Command completed\n");

    // Step 7: Send more commands
    handle
        .send_command_async(TaskCommand::IncrementCounter)
        .await?;

    handle
        .send_command_async(TaskCommand::IncrementCounter)
        .await?;

    handle.send_command_async(TaskCommand::GetStatus).await?;

    // Give commands time to process before shutting down
    switchy_async::time::sleep(std::time::Duration::from_millis(100)).await;

    // Step 8: Shutdown the service gracefully
    println!("\n🛑 Shutting down service...");
    handle.shutdown()?;

    // Step 9: Wait for the service to complete
    // This ensures all cleanup is finished before exiting
    join_handle.await??;

    println!("\n=== Example completed successfully ===");

    Ok(())
}
