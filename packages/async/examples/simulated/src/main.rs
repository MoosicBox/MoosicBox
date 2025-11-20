#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Async Simulated Example
//!
//! A demonstration of concurrent task spawning and execution using the switchy async runtime
//! with simulation capabilities.
//!
//! ## What it demonstrates
//!
//! This example showcases:
//!
//! * **Task spawning** - Creating multiple concurrent tasks with `task::spawn`
//! * **Nested futures** - Spawning tasks from within other tasks
//! * **Random delays** - Using random number generation with async sleep
//! * **Concurrent execution** - Multiple tasks running simultaneously
//! * **Runtime management** - Proper runtime lifecycle and cleanup
//! * **Simulation support** - Deterministic execution with seeded randomness
//!
//! ## Example output
//!
//! The program spawns 5 concurrent tasks with random sleep durations. Each task spawns
//! an additional nested task with more random delays. The output shows timestamps
//! demonstrating concurrent execution:
//!
//! ```text
//! Begin Asynchronous Execution (seed=12345)
//! block on
//! Blocking Function Polled To Completion
//! Spawned Fn #00: Start 1634664688
//! Spawned Fn #01: Start 1634664688
//! ...
//! End of Asynchronous Execution
//! ```
//!
//! ## Running the example
//!
//! Basic execution:
//! ```bash
//! cargo run --package async_simulated
//! ```
//!
//! With simulation features for deterministic execution:
//! ```bash
//! cargo run --package async_simulated --features simulator
//! ```

use std::time::Duration;

use switchy_async::{Error, runtime::Runtime, task, time};
use switchy_random::{rng, simulator::initial_seed};
use switchy_time::now;

/// Entry point for the async simulated example.
///
/// Demonstrates concurrent task execution with random delays using the switchy async runtime.
/// Creates a runtime, spawns multiple concurrent tasks with nested futures, and waits for
/// completion.
///
/// # Errors
///
/// * Returns an error if the runtime fails to wait on spawned tasks
fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let runtime = Runtime::new();

    runtime.spawn(async {
        let seed = initial_seed();

        println!("Begin Asynchronous Execution (seed={seed})");
        time::sleep(Duration::from_millis(1)).await;
        // Create a random number generator so we can generate random numbers
        // A small function to generate the time in seconds when we call it.
        let time = || {
            now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        };

        // Spawn 5 different futures on our executor
        for i in 0..5 {
            // Generate the two numbers between 1 and 9. We'll spawn two futures
            // that will sleep for as many seconds as the random number creates
            let random = rng().gen_range(1..10);
            let random2 = rng().gen_range(1..10);

            // We now spawn a future onto the runtime from within our future
            task::spawn(async move {
                println!("Spawned Fn #{:02}: Start {}", i, time());
                // This future will sleep for a certain amount of time before
                // continuing execution
                time::sleep(Duration::from_millis(1000 * random)).await;
                // After the future waits for a while, it then spawns another
                // future before printing that it finished. This spawned future
                // then sleeps for a while and then prints out when it's done.
                // Since we're spawning futures inside futures, the order of
                // execution can change.
                task::spawn(async move {
                    time::sleep(Duration::from_millis(1000 * random2)).await;
                    println!("Spawned Fn #{:02}: Inner {}", i, time());
                });
                println!("Spawned Fn #{:02}: Ended {}", i, time());
            });
        }
    });
    runtime.block_on(async {
        println!("block on");
        // This sleeps longer than any of the spawned functions, but we poll
        // this to completion first even if we await here.
        time::sleep(Duration::from_millis(11000)).await;
        println!("Blocking Function Polled To Completion");
    });

    // We now wait on the runtime to complete each of the tasks that were
    // spawned before we exit the program
    runtime.wait()?;
    println!("End of Asynchronous Execution");

    // When all is said and done when we run this test we should get output that
    // looks somewhat like this (though in different orders in each execution):
    //
    // Begin Asynchronous Execution
    // Blocking Function Polled To Completion
    // Spawned Fn #00: Start 1634664688
    // Spawned Fn #01: Start 1634664688
    // Spawned Fn #02: Start 1634664688
    // Spawned Fn #03: Start 1634664688
    // Spawned Fn #04: Start 1634664688
    // Spawned Fn #01: Ended 1634664690
    // Spawned Fn #01: Inner 1634664691
    // Spawned Fn #04: Ended 1634664694
    // Spawned Fn #04: Inner 1634664695
    // Spawned Fn #00: Ended 1634664697
    // Spawned Fn #02: Ended 1634664697
    // Spawned Fn #03: Ended 1634664697
    // Spawned Fn #00: Inner 1634664698
    // Spawned Fn #03: Inner 1634664698
    // Spawned Fn #02: Inner 1634664702
    // End of Asynchronous Execution

    Ok(())
}
