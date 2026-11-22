//! `SIMULATOR_RUNS=forever` under the default `SIMULATOR_ON_FAILURE=stop`: the fleet runs
//! until a run fails, then ends, keeping only failed runs' results.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use common::{Recorder, TracedBank, finished_runs, set_env};
use simvar::observer::{RunKind, SimEvent};

#[test]
fn an_unbounded_fleet_runs_until_a_failure_stops_it() {
    set_env(&[
        ("SIMULATOR_RUNS", "forever"),
        ("SIMULATOR_MAX_PARALLEL", "2"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    let recorder = Recorder::install();

    let results = simvar::run_simulation(TracedBank { racy: true }).unwrap();

    let events = recorder.events();
    assert!(
        matches!(
            events.first(),
            Some(SimEvent::Started {
                runs: None,
                max_parallel: 2
            })
        ),
        "{:?}",
        events.first()
    );
    assert!(matches!(events.last(), Some(SimEvent::Finished)));
    assert!(!results.is_empty(), "no failure reported");
    assert!(
        results.iter().all(|result| !result.is_success()),
        "an unbounded fleet keeps only failures"
    );

    let runs = finished_runs(&events);
    assert!(runs.iter().all(|run| run.kind == RunKind::Fleet));
    assert!(runs.iter().any(|run| !run.success));

    let status = simvar::control().status();
    assert_eq!(status.active, 0);
    assert_eq!(status.completed, runs.len() as u64);
    assert_eq!(
        status.failed,
        runs.iter().filter(|run| !run.success).count() as u64
    );
    assert_eq!(status.failed, results.len() as u64);
}
