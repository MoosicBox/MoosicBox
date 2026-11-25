//! Checking a failed run in-process with `simvar::control().verify`: a run that fails again
//! is reported as reproduced, with its failure.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use common::{Recorder, TracedBank, finished_runs, set_env};
use simvar::observer::{RunKind, SimEvent, Verification};

#[test]
fn a_verify_job_reports_a_run_that_fails_again_as_reproduced() {
    set_env(&[
        ("SIMULATOR_RUNS", "200"),
        ("SIMULATOR_MAX_PARALLEL", "1"),
        ("SIMULATOR_SEED", "1"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    let recorder = Recorder::install();

    let results = simvar::run_simulation(TracedBank { racy: true }).unwrap();
    assert!(results.iter().any(|result| !result.is_success()));
    let failed = *finished_runs(&recorder.events())
        .iter()
        .find(|run| !run.success)
        .unwrap();

    let id = simvar::control()
        .verify(failed.seed, failed.epoch_offset)
        .unwrap();
    recorder.wait_until("the verification", |events| {
        events
            .iter()
            .any(|event| matches!(event, SimEvent::Verified { id: of, .. } if *of == id))
    });

    let events = recorder.events();
    // The job's run is a replay of the failed one, and fails the same way.
    let run = finished_runs(&events)
        .into_iter()
        .find(|run| run.id == id)
        .unwrap();
    assert_eq!(run.kind, RunKind::Replay);
    assert!(!run.success);
    assert_eq!(run.fingerprint, failed.fingerprint);
    let verification = events
        .iter()
        .find_map(|event| match event {
            SimEvent::Verified {
                id: of,
                verification,
            } if *of == id => Some(verification.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        matches!(verification, Verification::Failed(ref report) if !report.is_empty()),
        "{verification:?}"
    );
}
