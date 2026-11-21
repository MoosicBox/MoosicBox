#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Runs the bank simulation.

use simvar::run_simulation;
use simvar_threads_example::BankBootstrap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let results = run_simulation(BankBootstrap {
        nondeterministic: std::env::var("BANK_NONDETERMINISTIC").is_ok(),
        racy: std::env::var("BANK_RACY").is_ok(),
        blocking: std::env::var("BANK_BLOCKING").is_ok(),
    })?;
    for result in results.iter().filter(|r| !r.is_success()) {
        println!("{result}");
    }
    let failed = results.iter().filter(|r| !r.is_success()).count();
    println!("{} runs, {failed} failed", results.len());
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
