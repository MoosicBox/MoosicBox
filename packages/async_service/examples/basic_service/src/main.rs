#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic async service example demonstrating the core features of `moosicbox_async_service`.
//!
//! This example shows:
//! - Creating a service with sequential command processing
//! - Sending commands asynchronously
//! - Waiting for command completion
//! - Service lifecycle hooks (`on_start`, `on_shutdown`)
//! - Proper shutdown and cleanup

use moosicbox_async_service::{Arc, Duration, async_service_sequential, async_trait, log, sync};
use tokio::time::sleep;

// Step 1: Define command types
// These represent the operations your service can perform
#[derive(Debug)]
pub enum TaskCommand {
    /// Process some data (simulates work)
    ProcessTask { id: u32, data: String },
    /// Get current status of the service
    GetStatus,
    /// Simulate an expensive operation
    HeavyComputation { value: u32 },
}

// Step 2: Define service context
// This holds the mutable state of your service
pub struct TaskContext {
    pub tasks_processed: u32,
    pub status: String,
    pub computation_result: u32,
}

// Step 3: Generate the service using the macro
// This creates the Service, Handle, Processor trait, and Error types
// Using sequential processing - commands are processed one at a time in order
async_service_sequential!(TaskCommand, TaskContext);

// Step 4: Implement the Processor trait
// This defines how each command is processed
#[async_trait]
impl Processor for Service {
    type Error = Error;

    async fn process_command(
        ctx: Arc<sync::RwLock<TaskContext>>,
        command: TaskCommand,
    ) -> Result<(), Self::Error> {
        match command {
            TaskCommand::ProcessTask { id, data } => {
                println!("  [Service] Processing task {id}: {data}");

                // Simulate some async work
                sleep(Duration::from_millis(100)).await;

                // Update context
                {
                    let mut context = ctx.write().await;
                    context.tasks_processed += 1;
                    context.status = format!("Processed task {id}");
                }

                println!("  [Service] Task {id} completed");
            }
            TaskCommand::GetStatus => {
                let (status, tasks_processed, computation_result) = {
                    let context = ctx.read().await;
                    (
                        context.status.clone(),
                        context.tasks_processed,
                        context.computation_result,
                    )
                };
                println!(
                    "  [Service] Status: {status} | Tasks: {tasks_processed} | Result: {computation_result}"
                );
            }
            TaskCommand::HeavyComputation { value } => {
                println!("  [Service] Starting heavy computation with value {value}");

                // Simulate CPU-intensive work
                sleep(Duration::from_millis(200)).await;
                let result = value * value;

                {
                    let mut context = ctx.write().await;
                    context.computation_result = result;
                }

                println!("  [Service] Computation complete: {value}^2 = {result}");
            }
        }
        Ok(())
    }

    // Lifecycle hook: called when service starts
    async fn on_start(&mut self) -> Result<(), Self::Error> {
        println!("[Lifecycle] Service starting up...");
        Ok(())
    }

    // Lifecycle hook: called when service shuts down
    async fn on_shutdown(ctx: Arc<sync::RwLock<TaskContext>>) -> Result<(), Self::Error> {
        let tasks_processed = ctx.read().await.tasks_processed;
        println!(
            "[Lifecycle] Service shutting down. Final stats: {tasks_processed} tasks processed"
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Async Service Example ===\n");

    // Step 5: Create the service context
    let context = TaskContext {
        tasks_processed: 0,
        status: "Initialized".to_string(),
        computation_result: 0,
    };

    // Step 6: Create and configure the service
    println!("[Main] Creating service...");
    let service = Service::new(context).with_name("TaskProcessor");

    // Step 7: Get a handle before starting (for sending commands)
    let handle = service.handle();

    // Step 8: Start the service
    println!("[Main] Starting service...\n");
    let join_handle = service.start();

    // Give the service a moment to start
    sleep(Duration::from_millis(50)).await;

    // Step 9: Send commands asynchronously (fire and forget)
    println!("[Main] Sending async commands...");
    handle
        .send_command_async(TaskCommand::ProcessTask {
            id: 1,
            data: "First task".to_string(),
        })
        .await?;

    handle
        .send_command_async(TaskCommand::ProcessTask {
            id: 2,
            data: "Second task".to_string(),
        })
        .await?;

    // Step 10: Send command and wait for completion
    println!("\n[Main] Sending command and waiting for completion...");
    handle
        .send_command_and_wait_async(TaskCommand::HeavyComputation { value: 7 })
        .await?;
    println!("[Main] Heavy computation completed");

    // Step 11: Check status
    println!("\n[Main] Requesting status...");
    handle.send_command_async(TaskCommand::GetStatus).await?;

    // Give time for status to be printed
    sleep(Duration::from_millis(100)).await;

    // Step 12: Send more tasks to demonstrate queuing
    println!("\n[Main] Sending batch of tasks...");
    for i in 3..=5 {
        handle
            .send_command_async(TaskCommand::ProcessTask {
                id: i,
                data: format!("Batch task {i}"),
            })
            .await?;
    }

    // Wait for tasks to complete
    sleep(Duration::from_millis(400)).await;

    // Step 13: Final status check
    println!("\n[Main] Final status check...");
    handle
        .send_command_and_wait_async(TaskCommand::GetStatus)
        .await?;

    // Step 14: Shutdown the service
    println!("\n[Main] Shutting down service...");
    handle.shutdown()?;

    // Step 15: Wait for the service to finish
    join_handle.await??;

    println!("\n[Main] Service shutdown complete");
    println!("\n=== Example Complete ===");

    Ok(())
}
