//! Shrinking a failed run's thread schedule in-process with `simvar::control().shrink`, and
//! replaying the schedule it finds.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use std::process::Command;

use common::{Recorder, TracedBank, finished_runs, set_env};
use simvar::observer::{RunKind, SimEvent};

#[test]
fn a_shrink_job_reports_a_smaller_schedule_that_still_fails() {
    set_env(&[
        ("SIMULATOR_RUNS", "200"),
        ("SIMULATOR_MAX_PARALLEL", "1"),
        ("SIMULATOR_SEED", "1"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    let recorder = Recorder::install();

    // The default `SIMULATOR_ON_FAILURE=stop`: the first failure ends the fleet.
    let results = simvar::run_simulation(TracedBank { racy: true }).unwrap();
    assert!(results.iter().any(|result| !result.is_success()));
    let failed = *finished_runs(&recorder.events())
        .iter()
        .find(|run| !run.success)
        .unwrap();

    // The fleet is over, and a job still runs.
    let id = simvar::control()
        .shrink(failed.seed, failed.epoch_offset)
        .unwrap();
    recorder.wait_until("the shrink to finish", |events| {
        events
            .iter()
            .any(|event| matches!(event, SimEvent::Shrink { id: of, done: true, .. } if *of == id))
    });

    let events = recorder.events();
    // The job's own run fails as the original did.
    let run = finished_runs(&events)
        .into_iter()
        .find(|run| run.id == id)
        .unwrap();
    assert_eq!(run.kind, RunKind::Shrink);
    assert!(!run.success);
    assert_eq!(run.fingerprint, failed.fingerprint);

    let progress: Vec<(u64, u64, u64, bool, Option<String>)> = events
        .iter()
        .filter_map(|event| match event {
            SimEvent::Shrink {
                id: of,
                attempts,
                best_len,
                original_len,
                done,
                schedule,
            } if *of == id => Some((*attempts, *best_len, *original_len, *done, schedule.clone())),
            _ => None,
        })
        .collect();
    assert!(progress.len() > 1, "{progress:?}");
    for pair in progress.windows(2) {
        assert!(pair[1].0 >= pair[0].0, "attempts went down: {progress:?}");
        assert!(
            pair[1].1 <= pair[0].1,
            "the best schedule grew: {progress:?}"
        );
    }
    let (attempts, best_len, original_len, done, schedule) = progress.last().unwrap().clone();
    assert!(done);
    // Without `SIMULATOR_SHRINK`, a job makes up to 1000 shrinking replays.
    assert!((1..=1000).contains(&attempts), "{attempts}");
    assert!(best_len < original_len, "{progress:?}");
    assert!(progress.iter().all(|step| step.2 == original_len));
    assert!(
        progress[..progress.len() - 1]
            .iter()
            .all(|step| !step.3 && step.4.is_none())
    );
    let schedule = schedule.expect("a finished shrink of a failure carries its schedule");

    // The schedule replays the failure on its own.
    let output = Command::new(env!("CARGO_BIN_EXE_simvar_threads_example"))
        .env("BANK_RACY", "1")
        .env("NO_TUI", "1")
        .env("SIMULATOR_RUNS", "1")
        .env("SIMULATOR_SEED", failed.seed.to_string())
        .env("SIMULATOR_EPOCH_OFFSET", failed.epoch_offset.to_string())
        .env("SIMULATOR_SCHEDULE", &schedule)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "the shrunk schedule passed: {text}"
    );
    assert!(text.contains("money was created or destroyed"), "{text}");
}
