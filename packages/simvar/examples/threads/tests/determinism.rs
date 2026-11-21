//! Determinism verification on a multithreaded simulation.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::process::Command;

fn run(nondeterministic: bool) -> (bool, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_simvar_threads_example"));
    command
        .env("SIMULATOR_VERIFY_DETERMINISM", "1")
        .env("SIMULATOR_RUNS", "10")
        .env("SIMULATOR_MAX_PARALLEL", "2")
        .env("SIMULATOR_SEED", "7");
    if nondeterministic {
        command.env("BANK_NONDETERMINISTIC", "1");
    }
    let output = command.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

#[test]
fn a_deterministic_multithreaded_simulation_replays_identically() {
    let (success, output) = run(false);
    assert!(success, "{output}");
}

#[test]
fn real_time_leaking_into_a_run_is_caught() {
    let (success, output) = run(true);
    assert!(!success, "{output}");
    assert!(output.contains("non-deterministic simulation"), "{output}");
}

#[test]
fn spawn_blocking_work_replays_identically_across_steps() {
    let output = Command::new(env!("CARGO_BIN_EXE_simvar_threads_example"))
        .env("SIMULATOR_VERIFY_DETERMINISM", "1")
        .env("SIMULATOR_RUNS", "20")
        .env("SIMULATOR_MAX_PARALLEL", "2")
        .env("SIMULATOR_SEED", "11")
        .env("BANK_BLOCKING", "1")
        .env("NO_TUI", "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{text}");
    assert!(text.contains("20 runs, 0 failed"), "{text}");
}
