//! Cancellation example for the switchy_async runtime.
//!
//! This example demonstrates how to use `CancellationToken` to gracefully shut down
//! an async runtime when Ctrl+C is pressed. The example creates a long-running task
//! that sleeps indefinitely, and cancels it cleanly when the user interrupts the program.

use std::{sync::LazyLock, time::Duration};

use switchy_async::{Error, runtime::Runtime, time, util::CancellationToken};

/// Global cancellation token used to coordinate shutdown across the application.
static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

/// Handles the Ctrl+C signal by canceling the global cancellation token.
fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}

/// Entry point for the cancellation example.
///
/// Creates an async runtime and spawns a long-running task that sleeps indefinitely.
/// When Ctrl+C is pressed, the cancellation token is triggered and the runtime
/// shuts down gracefully.
///
/// # Errors
///
/// * Returns an error if the runtime fails to initialize or wait for completion
///
/// # Panics
///
/// * Panics if setting the Ctrl+C handler fails
fn main() -> Result<(), Error> {
    ctrlc::set_handler(ctrl_c).unwrap();

    pretty_env_logger::init();

    let runtime = Runtime::new();

    runtime.block_on(TOKEN.run_until_cancelled(async move {
        println!("Blocking Function. Press ctrl+c to exit");
        time::sleep(Duration::MAX).await;
        println!("Blocking Function Polled To Completion");
    }));
    println!("After block_on");

    runtime.wait()?;
    println!("Runtime shut down cleanly");

    Ok(())
}
