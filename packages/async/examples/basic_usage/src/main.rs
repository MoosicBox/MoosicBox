#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for the `switchy_async` runtime.
//!
//! This example demonstrates the fundamental concepts of creating and using
//! an async runtime, spawning tasks, and running async code to completion.

use std::time::Duration;

use switchy_async::{Builder, Error, task, time};

/// An example async function that performs some async work
async fn greet(name: &str) -> String {
    // Simulate some async work with a small delay
    time::sleep(Duration::from_millis(10)).await;
    format!("Hello, {name}!")
}

/// An example of spawning a background task
async fn background_worker(id: u32) {
    println!("Background worker {id} starting");
    time::sleep(Duration::from_millis(50)).await;
    println!("Background worker {id} completed");
}

fn main() -> Result<(), Error> {
    println!("=== Switchy Async Basic Usage Example ===\n");

    // Step 1: Create a new async runtime using the Builder pattern
    println!("1. Creating async runtime...");
    let runtime = Builder::new().build()?;
    println!("   Runtime created successfully\n");

    // Step 2: Run a simple async function using block_on
    println!("2. Running simple async function with block_on...");
    let greeting = runtime.block_on(async {
        // We can call async functions inside this block
        let message = greet("World").await;
        println!("   Received: {message}");
        message
    });
    println!("   Result: {greeting}\n");

    // Step 3: Spawn concurrent background tasks
    println!("3. Spawning concurrent background tasks...");
    runtime.block_on(async {
        // Spawn multiple tasks that run concurrently
        let handle1 = task::spawn(background_worker(1));
        let handle2 = task::spawn(background_worker(2));
        let handle3 = task::spawn(background_worker(3));

        // Wait for all tasks to complete
        let _ = handle1.await;
        let _ = handle2.await;
        let _ = handle3.await;

        println!("   All background tasks completed");
    });
    println!();

    // Step 4: Demonstrate task results
    println!("4. Getting results from spawned tasks...");
    runtime.block_on(async {
        // Spawn a task that returns a value
        let computation = task::spawn(async {
            time::sleep(Duration::from_millis(20)).await;
            42 * 2
        });

        // Await the task to get its result
        match computation.await {
            Ok(result) => println!("   Computation result: {result}"),
            Err(e) => println!("   Task failed: {e}"),
        }
    });
    println!();

    // Step 5: Clean shutdown
    println!("5. Shutting down runtime...");
    runtime.wait()?;
    println!("   Runtime shut down cleanly\n");

    println!("=== Example completed successfully ===");

    Ok(())
}
