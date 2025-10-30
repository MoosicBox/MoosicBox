#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Demonstrates simulation cancellation patterns using `simvar_utils`.
//!
//! This example shows how to:
//! - Run async simulations with cancellation support
//! - Use thread-local and global cancellation tokens
//! - Handle graceful shutdown in simulation scenarios
//! - Reset cancellation state for multiple simulation runs

use std::time::Duration;

use simvar_utils::{
    cancel_global_simulation, cancel_simulation, is_global_simulator_cancelled,
    is_simulator_cancelled, reset_global_simulator_cancellation_token,
    reset_simulator_cancellation_token, run_until_simulation_cancelled, worker_thread_id,
};
use switchy_async::{runtime::Runtime, task, time};

/// Simulates a long-running async operation.
///
/// This represents work that should be cancellable, such as:
/// - Processing data in a simulation
/// - Running test scenarios
/// - Performing I/O operations
async fn simulate_work(task_id: u32, duration_ms: u64) -> u32 {
    println!(
        "[Thread {}] Task {}: Starting work (duration: {}ms)",
        worker_thread_id(),
        task_id,
        duration_ms
    );

    // Simulate work with multiple checkpoints
    for i in 0..duration_ms / 100 {
        time::sleep(Duration::from_millis(100)).await;

        // Check for cancellation during work
        if is_simulator_cancelled() {
            println!(
                "[Thread {}] Task {}: Detected cancellation at checkpoint {}",
                worker_thread_id(),
                task_id,
                i
            );
            return 0;
        }
    }

    println!(
        "[Thread {}] Task {}: Work completed successfully",
        worker_thread_id(),
        task_id
    );
    task_id * 100
}

/// Demonstrates running a simulation with thread-local cancellation.
///
/// The simulation will be cancelled after a short delay, showing how
/// `run_until_simulation_cancelled` gracefully handles cancellation.
async fn example_1_thread_local_cancellation() {
    println!("\n=== Example 1: Thread-Local Cancellation ===\n");

    // Reset cancellation state for clean start
    reset_simulator_cancellation_token();

    // Spawn a task that will cancel the simulation after 250ms
    task::spawn(async {
        time::sleep(Duration::from_millis(250)).await;
        println!(
            "[Thread {}] Cancelling thread-local simulation",
            worker_thread_id()
        );
        cancel_simulation();
    });

    // Run simulation until cancelled
    let result = run_until_simulation_cancelled(async {
        simulate_work(1, 1000).await // Would take 1 second if not cancelled
    })
    .await;

    match result {
        Some(output) => println!("✓ Simulation completed with result: {output}"),
        None => println!("✗ Simulation was cancelled (as expected)"),
    }
}

/// Demonstrates running multiple tasks with global cancellation.
///
/// Global cancellation affects all threads, useful for:
/// - Shutting down entire test suites
/// - Emergency stops in simulations
/// - Coordinated cleanup across threads
async fn example_2_global_cancellation() {
    println!("\n=== Example 2: Global Cancellation ===\n");

    // Reset global cancellation state
    reset_global_simulator_cancellation_token();

    // Spawn multiple simulation tasks
    let task1 = task::spawn(async {
        let result = run_until_simulation_cancelled(async {
            simulate_work(2, 1500).await // 1.5 seconds
        })
        .await;
        println!("Task 1 result: {result:?}");
    });

    let task2 = task::spawn(async {
        let result = run_until_simulation_cancelled(async {
            simulate_work(3, 1500).await // 1.5 seconds
        })
        .await;
        println!("Task 2 result: {result:?}");
    });

    // Global cancellation task
    let canceller = task::spawn(async {
        time::sleep(Duration::from_millis(400)).await;
        println!(
            "[Thread {}] Triggering global cancellation",
            worker_thread_id()
        );
        cancel_global_simulation();
    });

    // Wait for all tasks
    let _ = task1.await;
    let _ = task2.await;
    let _ = canceller.await;

    println!("\nGlobal cancellation complete");
}

/// Demonstrates checking cancellation status without `run_until_simulation_cancelled`.
///
/// Shows manual cancellation checking, useful when:
/// - Integrating with existing code
/// - Need fine-grained control over cancellation points
/// - Working with synchronous code
async fn example_3_manual_cancellation_checks() {
    println!("\n=== Example 3: Manual Cancellation Checks ===\n");

    // Reset cancellation state
    reset_simulator_cancellation_token();

    // Start cancellation in background
    task::spawn(async {
        time::sleep(Duration::from_millis(150)).await;
        cancel_simulation();
    });

    // Manually check cancellation in a loop
    let mut iterations = 0;
    loop {
        if is_simulator_cancelled() {
            println!(
                "[Thread {}] Detected cancellation after {} iterations",
                worker_thread_id(),
                iterations
            );
            break;
        }

        if is_global_simulator_cancelled() {
            println!(
                "[Thread {}] Detected global cancellation after {} iterations",
                worker_thread_id(),
                iterations
            );
            break;
        }

        // Do some work
        time::sleep(Duration::from_millis(50)).await;
        iterations += 1;

        if iterations >= 10 {
            println!("[Thread {}] Completed all iterations", worker_thread_id());
            break;
        }
    }
}

/// Demonstrates resetting cancellation tokens for multiple simulation runs.
///
/// Shows how to:
/// - Clean up cancellation state between runs
/// - Run multiple simulations sequentially
/// - Ensure each run starts with clean state
async fn example_4_reset_and_rerun() {
    println!("\n=== Example 4: Reset and Multiple Runs ===\n");

    for run in 1..=3 {
        println!("--- Run {run} ---");

        // Important: Reset cancellation state before each run
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();

        let result =
            run_until_simulation_cancelled(async { simulate_work(10 + run, 200).await }).await;

        match result {
            Some(output) => println!("Run {run} completed: {output}"),
            None => println!("Run {run} was cancelled"),
        }

        // Small delay between runs
        time::sleep(Duration::from_millis(100)).await;
    }
}

fn main() -> Result<(), switchy_async::Error> {
    pretty_env_logger::init();

    let runtime = Runtime::new();

    runtime.block_on(async {
        println!("Simulation Cancellation Examples");
        println!("=================================\n");
        println!("Worker Thread ID: {}", worker_thread_id());

        // Run all examples
        example_1_thread_local_cancellation().await;
        example_2_global_cancellation().await;
        example_3_manual_cancellation_checks().await;
        example_4_reset_and_rerun().await;

        println!("\n=================================");
        println!("All examples completed!");
    });

    runtime.wait()?;

    Ok(())
}
