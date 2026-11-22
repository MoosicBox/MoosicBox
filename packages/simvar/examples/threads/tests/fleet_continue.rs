//! `SIMULATOR_ON_FAILURE=continue`: failures are recorded and the fleet keeps going until it
//! is stopped.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use std::sync::Arc;

use common::{TracedBank, finished_runs, set_env};
use simvar::observer::{SimEvent, SimObserver};

/// Stops the simulation at its third failure.
struct StopAtThirdFailure(std::sync::atomic::AtomicU64);

impl SimObserver for StopAtThirdFailure {
    fn event(&self, event: SimEvent) {
        if let SimEvent::RunFinished { success: false, .. } = event
            && self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == 3
        {
            simvar::control().stop();
        }
    }
}

#[test]
fn a_continuing_fleet_keeps_running_past_failures() {
    set_env(&[
        ("SIMULATOR_RUNS", "forever"),
        ("SIMULATOR_ON_FAILURE", "continue"),
        ("SIMULATOR_MAX_PARALLEL", "2"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    let recorder = common::Recorder::install();
    simvar::add_observer(Arc::new(StopAtThirdFailure(0.into())));

    let results = simvar::run_simulation(TracedBank { racy: true }).unwrap();

    let runs = finished_runs(&recorder.events());
    let failures = runs.iter().filter(|run| !run.success).count();
    assert!(failures >= 3, "{runs:?}");
    // Runs kept starting after the first failure.
    let first_failure = runs.iter().position(|run| !run.success).unwrap();
    assert!(runs.len() > first_failure + 1, "{runs:?}");
    // Every failure is reported, and only failures (the fleet is unbounded).
    assert_eq!(results.len(), failures);
    assert!(results.iter().all(|result| !result.is_success()));
    let status = simvar::control().status();
    assert_eq!(status.failed, failures as u64);
    assert_eq!(status.completed, runs.len() as u64);
}
