//! Shrinking a failing multithreaded simulation's schedule, and replaying the result.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::process::Command;

fn bank(env: &[(&str, &str)]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_simvar_threads_example"))
        .env("BANK_RACY", "1")
        .env("NO_TUI", "1")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

/// The value of `name=` in the line `line` (up to the next space or backtick).
fn value<'a>(line: &'a str, name: &str) -> &'a str {
    let start = line.find(&format!("{name}=")).unwrap() + name.len() + 1;
    line[start..].split([' ', '`']).next().unwrap()
}

#[test]
fn a_racy_failure_shrinks_to_a_schedule_that_replays_it() {
    let (success, output) = bank(&[
        ("SIMULATOR_SHRINK", "1"),
        ("SIMULATOR_RUNS", "200"),
        ("SIMULATOR_MAX_PARALLEL", "1"),
        ("SIMULATOR_SEED", "1"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    assert!(!success, "the racy bank never lost money: {output}");
    assert!(
        output.contains("money was created or destroyed"),
        "{output}"
    );
    let line = output
        .lines()
        .find(|line| line.starts_with("shrunk the thread schedule"))
        .unwrap_or_else(|| panic!("no shrink report: {output}"));
    let counts: Vec<usize> = line
        .split_whitespace()
        .filter_map(|word| word.trim_end_matches(';').parse().ok())
        .collect();
    assert!(counts[1] < counts[0], "{line}");

    let (seed, schedule) = (
        value(line, "SIMULATOR_SEED"),
        value(line, "SIMULATOR_SCHEDULE"),
    );
    let (success, replay) = bank(&[
        ("SIMULATOR_SEED", seed),
        ("SIMULATOR_RUNS", "1"),
        ("SIMULATOR_SCHEDULE", schedule),
    ]);
    assert!(!success, "the shrunk schedule passed: {replay}");
    assert!(
        replay.contains("money was created or destroyed"),
        "{replay}"
    );
}
