#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic async runtime example demonstrating switchy's runtime abstraction.
//!
//! This example shows how to use switchy's async runtime abstractions that work
//! with both Tokio and simulator backends. The runtime is selected via feature flags.

use std::time::Instant;

#[cfg(feature = "async-macros")]
use switchy::unsync::{join, select, try_join};
use switchy::unsync::{
    task::spawn,
    time::{Duration, sleep},
};

/// Demonstrates basic async sleep operations
async fn demonstrate_sleep() {
    println!("=== Sleep Operations ===");

    // Sleep for 1 second
    println!("Sleeping for 1 second...");
    let start = Instant::now();
    sleep(Duration::from_secs(1)).await;
    let elapsed = start.elapsed();
    println!("Slept for {elapsed:?}");

    // Sleep for 500 milliseconds
    println!("\nSleeping for 500 milliseconds...");
    let start = Instant::now();
    sleep(Duration::from_millis(500)).await;
    let elapsed = start.elapsed();
    println!("Slept for {elapsed:?}");
}

/// Demonstrates spawning concurrent tasks
async fn demonstrate_spawn() {
    println!("\n=== Spawning Tasks ===");

    // Spawn a task that completes quickly
    let handle1 = spawn(async {
        sleep(Duration::from_millis(100)).await;
        println!("Task 1 completed");
        42
    });

    // Spawn a task that takes longer
    let handle2 = spawn(async {
        sleep(Duration::from_millis(200)).await;
        println!("Task 2 completed");
        100
    });

    // Wait for both tasks to complete
    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();

    println!("Task 1 result: {result1}");
    println!("Task 2 result: {result2}");
    println!("Sum of results: {}", result1 + result2);
}

/// Demonstrates `join!` macro for concurrent execution
#[cfg(feature = "async-macros")]
async fn demonstrate_join() {
    println!("\n=== Join Operations ===");

    // Execute multiple operations concurrently and wait for all to complete
    let (result1, result2, result3) = join!(
        async {
            sleep(Duration::from_millis(100)).await;
            println!("Join operation 1 completed");
            "first"
        },
        async {
            sleep(Duration::from_millis(150)).await;
            println!("Join operation 2 completed");
            "second"
        },
        async {
            sleep(Duration::from_millis(50)).await;
            println!("Join operation 3 completed");
            "third"
        }
    );

    println!("Results: {result1}, {result2}, {result3}");
}

/// Demonstrates `select!` macro for racing operations
#[cfg(feature = "async-macros")]
async fn demonstrate_select() {
    println!("\n=== Select Operations ===");

    // Race multiple operations and complete when the first one finishes
    select! {
        () = sleep(Duration::from_millis(100)) => {
            println!("First timer completed (100ms)");
        }
        () = sleep(Duration::from_millis(200)) => {
            println!("Second timer completed (200ms)");
        }
        () = async {
            sleep(Duration::from_millis(50)).await;
            println!("Async block completed (50ms)");
        } => {
            println!("Async block won the race!");
        }
    }
}

/// Demonstrates error handling with `try_join!`
#[cfg(feature = "async-macros")]
async fn demonstrate_try_join() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Try Join Operations ===");

    // Execute multiple fallible operations concurrently
    let (result1, result2) = try_join!(
        async {
            sleep(Duration::from_millis(100)).await;
            println!("Try join operation 1 succeeded");
            Ok::<_, std::io::Error>(42)
        },
        async {
            sleep(Duration::from_millis(150)).await;
            println!("Try join operation 2 succeeded");
            Ok::<_, std::io::Error>(100)
        }
    )?;

    println!("Results: {result1}, {result2}");
    println!("Sum: {}", result1 + result2);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Switchy Basic Async Example (Using Tokio Runtime)\n");

    demonstrate_sleep().await;
    demonstrate_spawn().await;

    #[cfg(feature = "async-macros")]
    {
        demonstrate_join().await;
        demonstrate_select().await;
        demonstrate_try_join().await?;
    }

    println!("\n=== Example Complete ===");
    println!("This example used the Tokio runtime backend.");
    println!("To use the simulator backend, compile with --features simulator");

    Ok(())
}
