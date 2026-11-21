//! Observing simvar simulations: events, telemetry and replay control, as plain data.
//!
//! A simulation harness sends [`SimEvent`]s to every registered [`SimObserver`]. A simulation
//! adds its own [`SimEvent::Telemetry`] (for example a [`SchedulerView`] of its threads), and
//! an observer can hold a run at scheduling decisions through a shared [`ReplayControl`].
//!
//! Observing never changes a run: events only report state, and pausing only stops real time.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    any::Any,
    fmt,
    sync::{Arc, Condvar, Mutex, PoisonError},
};

/// Identifies one run: the worker thread running it and its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId {
    /// The worker running it.
    pub worker: u64,
    /// The run's number, from 1.
    pub run: u64,
}

/// Something that happened in a simulation.
#[derive(Clone)]
#[non_exhaustive]
pub enum SimEvent {
    /// The simulation started.
    Started {
        /// How many runs it will make.
        runs: u64,
        /// How many runs it makes at once.
        max_parallel: u64,
    },
    /// A run started.
    RunStarted {
        /// Which run.
        id: RunId,
        /// Its seed.
        seed: u64,
        /// Its configuration and the simulation's properties, as name and value.
        props: Vec<(String, String)>,
        /// Its length in steps, when it has a fixed duration.
        steps: Option<u64>,
    },
    /// A run advanced.
    Progress {
        /// Which run.
        id: RunId,
        /// The simulation step.
        step: u64,
        /// The fraction done, when the run has a fixed duration.
        progress: Option<f64>,
    },
    /// A run ended.
    RunFinished {
        /// Which run.
        id: RunId,
        /// Its seed.
        seed: u64,
        /// Whether it succeeded.
        success: bool,
        /// The harness's report of the run, including how to replay it.
        report: String,
        /// The steps it ran.
        steps: u64,
    },
    /// Data a simulation emitted, such as a [`SchedulerView`]; observers downcast it.
    Telemetry {
        /// Which run.
        id: RunId,
        /// The data.
        payload: Arc<dyn Any + Send + Sync>,
    },
    /// Every run is done.
    Finished,
}

impl fmt::Debug for SimEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Started { runs, max_parallel } => {
                write!(f, "Started({runs} runs, {max_parallel} at once)")
            }
            Self::RunStarted { id, seed, .. } => {
                write!(f, "RunStarted({id:?}, seed {seed})")
            }
            Self::Progress { id, step, .. } => write!(f, "Progress({id:?}, step {step})"),
            Self::RunFinished {
                id, seed, success, ..
            } => write!(f, "RunFinished({id:?}, seed {seed}, success {success})"),
            Self::Telemetry { id, .. } => write!(f, "Telemetry({id:?})"),
            Self::Finished => f.write_str("Finished"),
        }
    }
}

/// Receives a simulation's events. Called from the simulation's threads, so it must return
/// quickly; queue the event and handle it elsewhere.
pub trait SimObserver: Send + Sync {
    /// Handle one event.
    fn event(&self, event: SimEvent);
}

/// One simulated thread, as an observer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadView {
    /// Its name.
    pub name: String,
    /// What it is doing: running, runnable, blocked on something, finished.
    pub state: String,
    /// The operation it last stopped at.
    pub op: String,
}

/// One scheduling decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionView {
    /// Its number in the run.
    pub number: u64,
    /// The thread that stopped, and where.
    pub from: String,
    /// The operation it stopped at.
    pub op: String,
    /// The thread chosen to run next.
    pub to: String,
    /// The simulation step it happened at.
    pub step: u64,
}

/// A run's scheduler: its threads and recent decisions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulerView {
    /// Every thread.
    pub threads: Vec<ThreadView>,
    /// The thread running.
    pub running: String,
    /// Decisions so far.
    pub decisions: u64,
    /// The running hash of every decision.
    pub trace_hash: u64,
    /// The most recent decisions, oldest first.
    pub recent: Vec<DecisionView>,
}

/// How a run under a [`ReplayControl`] proceeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// Run freely.
    Run,
    /// Stop at the next decision.
    Pause,
    /// Run this many more decisions, then pause.
    Step(u64),
    /// Run until this decision number, then pause.
    Until(u64),
}

/// Holds a run at scheduling decisions on an observer's behalf.
///
/// The run calls [`Self::checkpoint`] at each decision (blocking while paused); an observer
/// changes the mode. Pausing stops only real time: the run's decisions are the same either way.
#[derive(Debug)]
pub struct ReplayControl {
    state: Mutex<(ReplayMode, u64)>,
    changed: Condvar,
}

impl Default for ReplayControl {
    fn default() -> Self {
        Self::new(ReplayMode::Run)
    }
}

impl ReplayControl {
    /// A control starting in `mode`.
    #[must_use]
    pub const fn new(mode: ReplayMode) -> Self {
        Self {
            state: Mutex::new((mode, 0)),
            changed: Condvar::new(),
        }
    }

    /// Change how the run proceeds.
    pub fn set(&self, mode: ReplayMode) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0 = mode;
        self.changed.notify_all();
    }

    /// The current mode and the last decision the run reached.
    #[must_use]
    pub fn status(&self) -> (ReplayMode, u64) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Called by the run at decision `number`: returns once the run may continue.
    pub fn checkpoint(&self, number: u64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.1 = number;
        loop {
            match state.0 {
                ReplayMode::Run => return,
                ReplayMode::Step(0) => state.0 = ReplayMode::Pause,
                ReplayMode::Step(left) => {
                    state.0 = ReplayMode::Step(left - 1);
                    return;
                }
                ReplayMode::Until(target) if number < target => return,
                ReplayMode::Until(_) => state.0 = ReplayMode::Pause,
                ReplayMode::Pause => {}
            }
            self.changed.notify_all();
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_runs_that_many_decisions_then_pauses() {
        let control = Arc::new(ReplayControl::new(ReplayMode::Step(3)));
        for n in 0..3 {
            control.checkpoint(n);
        }
        let waiting = Arc::clone(&control);
        let blocked = std::thread::spawn(move || waiting.checkpoint(3));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(control.status(), (ReplayMode::Pause, 3));
        assert!(!blocked.is_finished());
        control.set(ReplayMode::Run);
        blocked.join().unwrap();
    }

    #[test]
    fn running_until_a_decision_pauses_there() {
        let control = ReplayControl::new(ReplayMode::Until(5));
        for n in 0..5 {
            control.checkpoint(n);
        }
        assert_eq!(control.status().0, ReplayMode::Until(5));
        let control = Arc::new(control);
        let waiting = Arc::clone(&control);
        let blocked = std::thread::spawn(move || waiting.checkpoint(5));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(control.status().0, ReplayMode::Pause);
        control.set(ReplayMode::Run);
        blocked.join().unwrap();
    }
}
