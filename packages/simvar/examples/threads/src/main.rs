#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Runs the bank simulation.

use simvar::run_simulation;
use simvar_threads_example::BankBootstrap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nondeterministic = std::env::var("BANK_NONDETERMINISTIC").is_ok();
    let results = run_simulation(BankBootstrap { nondeterministic })?;
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
