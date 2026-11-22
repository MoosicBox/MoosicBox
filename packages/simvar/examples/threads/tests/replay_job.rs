//! Replaying a finished run in-process with `simvar::control().replay`: the replay does
//! exactly what the run did.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use common::{Recorder, TracedBank, finished_runs, set_env};
use simvar::{
    ReplayRequest,
    observer::{RunId, RunKind},
};

#[test]
fn a_replay_job_reproduces_its_run_exactly() {
    set_env(&[
        ("SIMULATOR_RUNS", "12"),
        ("SIMULATOR_ON_FAILURE", "continue"),
        ("SIMULATOR_MAX_PARALLEL", "3"),
        ("SIMULATOR_SCHEDULE_STRATEGY", "random"),
    ]);
    let recorder = Recorder::install();

    // Nothing to replay before a simulation starts.
    assert!(
        simvar::control()
            .replay(ReplayRequest {
                seed: 1,
                epoch_offset: 0,
                control: None,
            })
            .is_err()
    );

    let results = simvar::run_simulation(TracedBank { racy: true }).unwrap();
    assert_eq!(results.len(), 12);

    let originals = finished_runs(&recorder.events());
    assert_eq!(originals.len(), 12);
    assert!(originals.iter().all(|run| run.kind == RunKind::Fleet));
    // The runs differ from each other, so matching a replay to its run means something.
    let distinct: std::collections::BTreeSet<u64> = originals
        .iter()
        .map(|run| run.fingerprint.unwrap().trace_hash)
        .collect();
    assert!(distinct.len() > 1, "{originals:?}");

    // Replay them all at once, after the fleet finished.
    let control = simvar::control();
    let jobs: Vec<(RunId, common::Run)> = originals
        .iter()
        .map(|run| {
            let id = control
                .replay(ReplayRequest {
                    seed: run.seed,
                    epoch_offset: run.epoch_offset,
                    control: None,
                })
                .unwrap();
            (id, *run)
        })
        .collect();
    recorder.wait_until("every replay", |events| {
        let finished = finished_runs(events);
        jobs.iter()
            .all(|(id, _)| finished.iter().any(|run| run.id == *id))
    });

    let finished = finished_runs(&recorder.events());
    for (id, original) in jobs {
        assert_eq!(id.worker, 0, "jobs run outside the fleet");
        let replay = finished.iter().find(|run| run.id == id).unwrap();
        assert_eq!(replay.kind, RunKind::Replay);
        assert_eq!(replay.worker, 0);
        assert_eq!(replay.seed, original.seed);
        assert_eq!(replay.epoch_offset, original.epoch_offset);
        assert_eq!(
            replay.success, original.success,
            "{original:?} -> {replay:?}"
        );
        assert_eq!(
            replay.fingerprint, original.fingerprint,
            "{original:?} -> {replay:?}"
        );
    }

    // Jobs never count toward the fleet.
    let status = control.status();
    assert_eq!(status.completed, 12);
}
