//! The replicated counter survives its nemesis, and replays identically.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::process::Command;

#[test]
fn the_replicas_converge_through_partitions_and_bounces_deterministically() {
    let output = Command::new(env!("CARGO_BIN_EXE_simvar_nemesis_example"))
        .env("SIMULATOR_VERIFY_DETERMINISM", "1")
        .env("SIMULATOR_RUNS", "8")
        .env("SIMULATOR_MAX_PARALLEL", "2")
        .env("SIMULATOR_SEED", "11")
        .env("NO_TUI", "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{text}");
    assert!(text.contains("8 runs, 0 failed"), "{text}");
}
