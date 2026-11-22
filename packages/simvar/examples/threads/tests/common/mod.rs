//! Shared by the fleet control tests: a bank that reports a fingerprint of each run, and an
//! observer that keeps every event.
//!
//! Each test file holds a single test: the harness reads its settings from the environment
//! once per process, so every test sets them first, in a process of its own.

// Each test binary uses only part of this module.
#![allow(dead_code)]

use std::{
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::{Duration, Instant, UNIX_EPOCH},
};

use simvar::{
    Sim, SimBootstrap, SimConfig,
    observer::{RunId, RunKind, SimEvent, SimObserver},
};
use simvar_threads_example::{ACCOUNTS, STARTING_BALANCE, run_bank};

/// What a run did, as the bank reports it at its end: two runs that did the same thing
/// report the same fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    /// The run's thread scheduler's hash of every decision.
    pub trace_hash: u64,
    /// The scheduling decisions made.
    pub decisions: u64,
    /// The run's simulated wall clock, in milliseconds since the Unix epoch.
    pub now_millis: u128,
    /// The bank's final total.
    pub total: u64,
}

/// The bank, reporting a [`Fingerprint`] of each run as telemetry before checking it.
#[derive(Debug, Clone, Copy)]
pub struct TracedBank {
    /// Lose updates under some interleavings (see `BankBootstrap::racy`).
    pub racy: bool,
}

impl SimBootstrap for TracedBank {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(1);
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        let racy = self.racy;
        sim.client("bank", async move {
            let total: u64 = run_bank(false, racy).iter().sum();
            let scheduler = switchy_thread::simulator::scheduler();
            simvar::emit(Arc::new(Fingerprint {
                trace_hash: scheduler.trace_hash(),
                decisions: scheduler.decisions(),
                now_millis: simvar::switchy::time::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                total,
            }));
            assert_eq!(
                total,
                STARTING_BALANCE * ACCOUNTS as u64,
                "money was created or destroyed"
            );
            Ok(())
        });
    }
}

/// Keeps every event (telemetry other than a [`Fingerprint`] is dropped) and lets a test
/// wait for one.
#[derive(Default)]
pub struct Recorder {
    events: Mutex<Vec<SimEvent>>,
    changed: Condvar,
}

impl SimObserver for Recorder {
    fn event(&self, event: SimEvent) {
        if let SimEvent::Telemetry { payload, .. } = &event
            && !payload.is::<Fingerprint>()
        {
            return;
        }
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
        self.changed.notify_all();
    }
}

impl Recorder {
    /// A recorder registered with the harness.
    pub fn install() -> Arc<Self> {
        let recorder = Arc::new(Self::default());
        simvar::add_observer(recorder.clone());
        recorder
    }

    /// Every event so far.
    pub fn events(&self) -> Vec<SimEvent> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Wait (up to a minute) until `done` holds for the events so far.
    ///
    /// # Panics
    ///
    /// * If it does not within a minute
    pub fn wait_until(&self, what: &str, done: impl Fn(&[SimEvent]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        while !done(&events) {
            let left = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("timed out waiting for {what}"));
            events = self
                .changed
                .wait_timeout(events, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        drop(events);
    }
}

/// A finished run as the events describe it.
#[derive(Debug, Clone, Copy)]
pub struct Run {
    pub id: RunId,
    pub kind: RunKind,
    pub worker: u64,
    pub seed: u64,
    pub epoch_offset: u64,
    pub success: bool,
    pub fingerprint: Option<Fingerprint>,
}

/// Every finished run, in the order they finished.
pub fn finished_runs(events: &[SimEvent]) -> Vec<Run> {
    events
        .iter()
        .filter_map(|event| match event {
            SimEvent::RunFinished { id, success, .. } => Some((*id, *success)),
            _ => None,
        })
        .map(|(id, success)| {
            let (kind, worker, seed, epoch_offset) = events
                .iter()
                .find_map(|event| match event {
                    SimEvent::RunStarted {
                        id: started,
                        kind,
                        worker,
                        seed,
                        epoch_offset,
                        ..
                    } if *started == id => Some((*kind, *worker, *seed, *epoch_offset)),
                    _ => None,
                })
                .expect("a finished run started");
            let fingerprint = events.iter().find_map(|event| match event {
                SimEvent::Telemetry { id: of, payload } if *of == id => {
                    payload.downcast_ref::<Fingerprint>().copied()
                }
                _ => None,
            });
            Run {
                id,
                kind,
                worker,
                seed,
                epoch_offset,
                success,
                fingerprint,
            }
        })
        .collect()
}

/// Set the harness's environment. Call before anything else in the test.
pub fn set_env(vars: &[(&str, &str)]) {
    for (name, value) in vars.iter().copied().chain([("NO_TUI", "1")]) {
        // SAFETY: every test sets its environment first, before starting any thread.
        unsafe { std::env::set_var(name, value) };
    }
}
