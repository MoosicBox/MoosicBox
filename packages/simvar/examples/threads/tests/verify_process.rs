//! A verify job's replay in a fresh process (`simvar::control().verify`): a deterministic run
//! and its replay agree.
//!
//! The replay runs this program again, as it would a simulation's own binary, so this test has
//! its own `main` (`harness = false`): started by the job (`SIMULATOR_DIGEST_OUT` set), it runs
//! the one run asked for and reports what it observed.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use common::{Recorder, TracedBank, finished_runs, set_env};
use simvar::observer::{SimEvent, Verification};

fn main() {
    if std::env::var_os("SIMULATOR_DIGEST_OUT").is_some() {
        // The replay: the job set this run's seed and epoch offset.
        simvar::run_simulation(TracedBank { racy: false }).unwrap();
        return;
    }
    set_env(&[
        ("SIMULATOR_RUNS", "3"),
        ("SIMULATOR_MAX_PARALLEL", "1"),
        ("SIMULATOR_SEED", "7"),
    ]);
    let recorder = Recorder::install();
    let results = simvar::run_simulation(TracedBank { racy: false }).unwrap();
    assert!(results.iter().all(simvar::SimResult::is_success));
    let run = finished_runs(&recorder.events())[0];

    let id = simvar::control()
        .verify(run.seed, run.epoch_offset)
        .unwrap();
    recorder.wait_until("the verification", |events| {
        events
            .iter()
            .any(|event| matches!(event, SimEvent::Verified { id: of, .. } if *of == id))
    });
    let verification = recorder
        .events()
        .iter()
        .find_map(|event| match event {
            SimEvent::Verified {
                id: of,
                verification,
            } if *of == id => Some(verification.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(verification, Verification::Same);
    println!("verify_process: a deterministic run and its replay in a fresh process agree");
}
