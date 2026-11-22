#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Runs the replicated counter under a nemesis.

use simvar::run_simulation;
use simvar_nemesis_example::CounterBootstrap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let results = run_simulation(CounterBootstrap)?;
    let failed = results.iter().filter(|r| !r.is_success()).count();
    for result in results.iter().filter(|r| !r.is_success()) {
        println!("{result}");
    }
    println!("{} runs, {failed} failed", results.len());
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
