#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example demonstrating fundamental `switchy_async` patterns.
//!
//! This example shows:
//! - Creating a runtime with the Builder API
//! - Running async operations with `block_on`
//! - Spawning background tasks
//! - Awaiting task results
//! - Proper runtime cleanup

use std::time::Duration;

use switchy_async::{Builder, Error, task, time};

fn main() -> Result<(), Error> {
    // Initialize logging for debugging
    pretty_env_logger::init();

    println!("=== Switchy Async Basic Usage Example ===\n");

    // Step 1: Create a runtime using the Builder pattern
    println!("1. Creating runtime with Builder...");
    let runtime = Builder::new().build()?;
    println!("   Runtime created successfully\n");

    // Step 2: Run a simple async operation with block_on
    println!("2. Running simple async operation with block_on...");
    let result = runtime.block_on(async {
        println!("   Inside async block");
        time::sleep(Duration::from_millis(100)).await;
        println!("   After sleep");
        42
    });
    println!("   Result: {result}\n");

    // Step 3: Spawn a background task and await its result
    println!("3. Spawning background task...");
    runtime.block_on(async {
        // Spawn a task that runs in the background
        let handle = task::spawn(async {
            println!("   Background task started");
            time::sleep(Duration::from_millis(200)).await;
            println!("   Background task completing");
            "task result"
        });

        // Do other work while the background task runs
        println!("   Main async block doing other work");
        time::sleep(Duration::from_millis(100)).await;

        // Await the background task's result
        let task_result = handle.await.expect("Task should complete successfully");
        println!("   Background task result: {task_result}\n");
    });

    // Step 4: Spawn multiple concurrent tasks
    println!("4. Spawning multiple concurrent tasks...");
    runtime.block_on(async {
        let mut handles = Vec::new();

        // Spawn 3 concurrent tasks
        for i in 1_u64..=3 {
            let handle = task::spawn(async move {
                println!("   Task {i} starting");
                time::sleep(Duration::from_millis(50 * i)).await;
                println!("   Task {i} completing");
                i * 10
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        println!("   Waiting for all tasks to complete...");
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.expect("Task should complete");
            println!("   Task {} result: {}", i + 1, result);
        }
        println!();
    });

    // Step 5: Demonstrate task spawning with shared state
    println!("5. Demonstrating task coordination...");
    runtime.block_on(async {
        // Spawn a producer task
        let producer = task::spawn(async {
            println!("   Producer: generating data");
            time::sleep(Duration::from_millis(100)).await;
            println!("   Producer: data ready");
            vec![1, 2, 3, 4, 5]
        });

        // Spawn a consumer task that waits for the producer
        let consumer = task::spawn(async move {
            println!("   Consumer: waiting for data");
            let data = producer.await.expect("Producer should complete");
            println!("   Consumer: received {} items", data.len());
            let sum: i32 = data.iter().sum();
            println!("   Consumer: sum = {sum}");
            sum
        });

        let final_result = consumer.await.expect("Consumer should complete");
        println!("   Final result: {final_result}\n");
    });

    // Step 6: Clean up the runtime
    println!("6. Cleaning up runtime...");
    runtime.wait()?;
    println!("   Runtime shut down successfully\n");

    println!("=== Example completed successfully ===");

    Ok(())
}
