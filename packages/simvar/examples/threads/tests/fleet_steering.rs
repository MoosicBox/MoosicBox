//! Pausing, resuming and resizing a running fleet through `simvar::control()`.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod common;

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use common::{Recorder, TracedBank, set_env};
use simvar::{
    FleetStatus,
    observer::{RunId, SimEvent},
};

fn finished(events: &[SimEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, SimEvent::RunFinished { .. }))
        .count()
}

fn started(events: &[SimEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, SimEvent::RunStarted { .. }))
        .count()
}

/// The most runs in progress at once among `events`, and the worker slots that ran them.
fn concurrency(events: &[SimEvent]) -> (usize, BTreeSet<u64>) {
    let mut running = BTreeSet::<RunId>::new();
    let mut most = 0;
    let mut slots = BTreeSet::new();
    for event in events {
        match event {
            SimEvent::RunStarted { id, worker, .. } => {
                running.insert(*id);
                slots.insert(*worker);
                most = most.max(running.len());
            }
            SimEvent::RunFinished { id, .. } => {
                running.remove(id);
            }
            _ => {}
        }
    }
    (most, slots)
}

/// Wait (up to a minute) until the fleet's status satisfies `done`.
fn wait_for_status(what: &str, done: impl Fn(&FleetStatus) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(&simvar::control().status()) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_running_fleet_pauses_resumes_and_resizes() {
    set_env(&[
        ("SIMULATOR_RUNS", "forever"),
        ("SIMULATOR_MAX_PARALLEL", "2"),
    ]);
    let recorder = Recorder::install();
    let control = simvar::control();
    let fleet = std::thread::spawn(|| {
        simvar::run_simulation(TracedBank { racy: false }).map_err(|e| e.to_string())
    });

    recorder.wait_until("five runs", |events| finished(events) >= 5);
    let (most, slots) = concurrency(&recorder.events());
    assert!(most <= 2, "{most} runs at once with two workers");
    assert!(slots.iter().all(|slot| (1..=2).contains(slot)), "{slots:?}");

    // Paused: runs in progress finish and no new one starts.
    control.pause();
    // A run a worker took just before the pause may not have reported its start yet, so
    // the fleet's own count says when it drained.
    wait_for_status("the paused fleet to drain", |status| status.active == 0);
    let status = control.status();
    assert_eq!(started(&recorder.events()), finished(&recorder.events()));
    assert!(status.paused);
    assert_eq!(status.active, 0);
    let before = recorder.events().len();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        recorder.events().len(),
        before,
        "a run started while paused"
    );
    assert_eq!(control.status().completed, status.completed);

    // Grown to four workers while paused, then resumed: up to four runs at once.
    control.set_max_parallel(4);
    control.resume();
    let mark = recorder.events().len();
    recorder.wait_until("four runs at once", |events| {
        concurrency(&events[mark..]).0 == 4
    });
    let (_, slots) = concurrency(&recorder.events()[mark..]);
    assert_eq!(slots, (1..=4).collect(), "four workers use slots 1 to 4");

    // Shrunk to one: the extra workers retire as their runs end, then one run at a time.
    control.set_max_parallel(1);
    wait_for_status("the fleet to shrink", |status| status.active <= 1);
    let mark = recorder.events().len();
    recorder.wait_until("five more runs", |events| finished(&events[mark..]) >= 5);
    let (most, _) = concurrency(&recorder.events()[mark..]);
    assert_eq!(
        most, 1,
        "more than one run at once after shrinking to one worker"
    );
    assert_eq!(control.status().max_parallel, 1);

    control.stop();
    let results = fleet.join().unwrap().unwrap();
    assert!(results.is_empty(), "a correct bank failed: {results:?}");
    let status = control.status();
    let events = recorder.events();
    assert_eq!(status.active, 0, "{status:?}");
    assert_eq!(started(&events), finished(&events));
    assert_eq!(status.failed, 0);
}
