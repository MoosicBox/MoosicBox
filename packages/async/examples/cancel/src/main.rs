//! Cancellation example for the switchy_async runtime.
//!
//! This example demonstrates how to use `CancellationToken` to gracefully shut down
//! an async runtime when Ctrl+C is pressed. The example creates a long-running task
//! that sleeps indefinitely, and cancels it cleanly when the user interrupts the program.

use std::{sync::LazyLock, time::Duration};

use switchy_async::{Error, runtime::Runtime, time, util::CancellationToken};

static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

/// Handles the Ctrl+C signal by canceling the global cancellation token.
fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}

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
