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
    time::Duration,
};

/// Identifies one run: the worker thread running it and its number.
///
/// Fleet runs carry their worker thread's id (from 1). Replay and shrink jobs run outside
/// the fleet and use worker `0`, with their own numbers from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId {
    /// The worker running it (`0` for a replay or shrink job).
    pub worker: u64,
    /// The run's number, from 1 (fleet runs and jobs are numbered separately).
    pub run: u64,
}

/// Why a run was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunKind {
    /// One of the simulation's own runs, counted in its totals.
    Fleet,
    /// An in-process replay of an earlier run's seed, started by an observer.
    Replay,
    /// The run a shrink job starts from (its shrinking replays report nothing but
    /// [`SimEvent::Shrink`]).
    Shrink,
}

/// Something that happened in a simulation.
#[derive(Clone)]
#[non_exhaustive]
pub enum SimEvent {
    /// The simulation started.
    Started {
        /// How many runs it will make, or `None` when it runs until stopped
        /// (`SIMULATOR_RUNS=forever`).
        runs: Option<u64>,
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
        /// The fleet worker slot running it, from 1 and reused as workers come and go
        /// (`0` for a replay or shrink job).
        worker: u64,
        /// Why it runs.
        kind: RunKind,
        /// Its simulated clock's epoch offset: with the seed, all a replay needs.
        epoch_offset: u64,
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
        /// The real (wall-clock) time it took.
        real_time: Duration,
        /// The simulated time that passed in it.
        sim_time: Duration,
    },
    /// A shrink job's progress: smaller failing thread schedules for one failed run.
    Shrink {
        /// The shrink job (the run it started from).
        id: RunId,
        /// Shrinking replays made so far.
        attempts: u64,
        /// Choices in the smallest failing schedule so far.
        best_len: u64,
        /// Choices in the failed run's own schedule.
        original_len: u64,
        /// Whether shrinking is over.
        done: bool,
        /// Once done, the smallest failing schedule in `SIMULATOR_SCHEDULE`'s format; `None`
        /// while shrinking, or when the run did not fail.
        schedule: Option<String>,
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
            Self::Started {
                runs: Some(runs),
                max_parallel,
            } => write!(f, "Started({runs} runs, {max_parallel} at once)"),
            Self::Started {
                runs: None,
                max_parallel,
            } => write!(f, "Started(unbounded, {max_parallel} at once)"),
            Self::RunStarted { id, seed, kind, .. } => {
                write!(f, "RunStarted({id:?}, {kind:?}, seed {seed})")
            }
            Self::Progress { id, step, .. } => write!(f, "Progress({id:?}, step {step})"),
            Self::RunFinished {
                id, seed, success, ..
            } => write!(f, "RunFinished({id:?}, seed {seed}, success {success})"),
            Self::Shrink {
                id,
                attempts,
                best_len,
                original_len,
                done,
                ..
            } => write!(
                f,
                "Shrink({id:?}, {attempts} attempts, {best_len}/{original_len} choices, done \
                 {done})"
            ),
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
    /// Run freely (until a breakpoint).
    Run,
    /// Stop at the next decision.
    Pause,
    /// Pause at the decision this many from now (at the next one for `Step(1)`).
    Step(u64),
    /// Run until this decision number, then pause.
    Until(u64),
}

/// Where a run under a [`ReplayControl`] pauses on its own. Each but [`Self::Failure`] pauses
/// once and is then removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Breakpoint {
    /// At this decision number.
    Decision(u64),
    /// At the first decision at or after this simulation step.
    Step(u64),
    /// When a thread whose name contains `thread` is blocked on something containing `on`
    /// (any wait, if empty).
    Blocked {
        /// Part of the thread's name.
        thread: String,
        /// Part of what it waits on.
        on: String,
    },
    /// Whenever the simulation reports a failure (see [`ReplayControl::failure`]).
    Failure,
}

impl fmt::Display for Breakpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decision(number) => write!(f, "decision {number}"),
            Self::Step(step) => write!(f, "step {step}"),
            Self::Blocked { thread, on } if on.is_empty() => write!(f, "{thread} blocks"),
            Self::Blocked { thread, on } => write!(f, "{thread} blocks on {on}"),
            Self::Failure => f.write_str("a failure"),
        }
    }
}

#[derive(Debug)]
struct ReplayState {
    mode: ReplayMode,
    decision: u64,
    breakpoints: Vec<Breakpoint>,
    hit: Option<String>,
}

/// Holds a run at scheduling decisions on an observer's behalf.
///
/// At each decision the run calls [`Self::arrive`], and if that says it stops, sends what an
/// observer should see and then calls [`Self::wait`] (or both at once with
/// [`Self::checkpoint`]). An observer changes the mode and the breakpoints. Pausing stops only
/// real time: the run's decisions are the same either way.
#[derive(Debug)]
pub struct ReplayControl {
    state: Mutex<ReplayState>,
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
            state: Mutex::new(ReplayState {
                mode,
                decision: 0,
                breakpoints: Vec::new(),
                hit: None,
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ReplayState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Change how the run proceeds.
    pub fn set(&self, mode: ReplayMode) {
        let mut state = self.lock();
        state.mode = mode;
        if mode != ReplayMode::Pause {
            state.hit = None;
        }
        drop(state);
        self.changed.notify_all();
    }

    /// The current mode and the last decision the run reached.
    #[must_use]
    pub fn status(&self) -> (ReplayMode, u64) {
        let state = self.lock();
        (state.mode, state.decision)
    }

    /// Why the run last paused on its own (a breakpoint, a failure or a target decision),
    /// until it runs again.
    #[must_use]
    pub fn hit(&self) -> Option<String> {
        self.lock().hit.clone()
    }

    /// Pause at `breakpoint` too.
    pub fn add_breakpoint(&self, breakpoint: Breakpoint) {
        let mut state = self.lock();
        if !state.breakpoints.contains(&breakpoint) {
            state.breakpoints.push(breakpoint);
        }
    }

    /// Remove every breakpoint.
    pub fn clear_breakpoints(&self) {
        self.lock().breakpoints.clear();
    }

    /// The breakpoints set.
    #[must_use]
    pub fn breakpoints(&self) -> Vec<Breakpoint> {
        self.lock().breakpoints.clone()
    }

    /// Called by the run at decision `number`, made at simulation step `step`; `view` gives
    /// its scheduler (taken only when a breakpoint needs it). Returns whether the run stops
    /// here: then it should send what an observer should see, and call [`Self::wait`].
    pub fn arrive(&self, number: u64, step: u64, view: &dyn Fn() -> SchedulerView) -> bool {
        let mut state = self.lock();
        state.decision = number;
        let reason = match state.mode {
            ReplayMode::Pause => return true,
            ReplayMode::Step(0 | 1) => Some(format!("stepped to decision {number}")),
            ReplayMode::Until(target) if number >= target => {
                Some(format!("reached decision {target}"))
            }
            ReplayMode::Step(left) => {
                state.mode = ReplayMode::Step(left - 1);
                None
            }
            ReplayMode::Run | ReplayMode::Until(_) => None,
        }
        .or_else(|| {
            let index = Self::matching(&state.breakpoints, number, step, view)?;
            Some(format!("break at {}", state.breakpoints.remove(index)))
        });
        let Some(reason) = reason else {
            return false;
        };
        state.mode = ReplayMode::Pause;
        state.hit = Some(reason);
        drop(state);
        true
    }

    /// The first one-shot breakpoint matching this decision.
    fn matching(
        breakpoints: &[Breakpoint],
        number: u64,
        step: u64,
        view: &dyn Fn() -> SchedulerView,
    ) -> Option<usize> {
        let mut scheduler = None;
        breakpoints.iter().position(|breakpoint| match breakpoint {
            Breakpoint::Decision(target) => number >= *target,
            Breakpoint::Step(target) => step >= *target,
            Breakpoint::Blocked { thread, on } => {
                scheduler.get_or_insert_with(view).threads.iter().any(|t| {
                    t.name.contains(thread.as_str())
                        && t.state.starts_with("Blocked")
                        && t.state.contains(on.as_str())
                })
            }
            Breakpoint::Failure => false,
        })
    }

    /// Called by the simulation when it finds a failure (`what`). Returns whether the run
    /// stops for it (a [`Breakpoint::Failure`] is set): then it should send what an observer
    /// should see, and call [`Self::wait`].
    pub fn failure(&self, what: &str) -> bool {
        let mut state = self.lock();
        if !state.breakpoints.contains(&Breakpoint::Failure) {
            return false;
        }
        state.mode = ReplayMode::Pause;
        state.hit = Some(format!("failure: {what}"));
        true
    }

    /// Block while the run is paused.
    pub fn wait(&self) {
        drop(
            self.changed
                .wait_while(self.lock(), |state| state.mode == ReplayMode::Pause)
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    /// [`Self::arrive`] then [`Self::wait`], for runs that send nothing when they stop.
    pub fn checkpoint(&self, number: u64) {
        if self.arrive(number, 0, &SchedulerView::default) {
            self.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_pauses_that_many_decisions_on() {
        let control = Arc::new(ReplayControl::new(ReplayMode::Step(3)));
        for n in 0..2 {
            control.checkpoint(n);
        }
        let waiting = Arc::clone(&control);
        let blocked = std::thread::spawn(move || waiting.checkpoint(2));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(control.status(), (ReplayMode::Pause, 2));
        assert!(!blocked.is_finished());
        // Stepping one from a pause stops at the very next decision.
        control.set(ReplayMode::Step(1));
        blocked.join().unwrap();
        assert!(control.arrive(3, 0, &SchedulerView::default));
        assert_eq!(control.hit().as_deref(), Some("stepped to decision 3"));
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

    fn blocked(name: &str, on: &str) -> SchedulerView {
        SchedulerView {
            threads: vec![ThreadView {
                name: name.into(),
                state: format!("Blocked {{ key: {on} }}"),
                op: String::new(),
            }],
            ..SchedulerView::default()
        }
    }

    #[test]
    fn breakpoints_pause_once_and_say_why() {
        let control = ReplayControl::default();
        control.add_breakpoint(Breakpoint::Decision(5));
        control.add_breakpoint(Breakpoint::Step(1000));
        let idle = SchedulerView::default;
        assert!(!control.arrive(4, 10, &idle));
        assert!(control.arrive(5, 10, &idle));
        assert_eq!(control.hit().as_deref(), Some("break at decision 5"));
        control.set(ReplayMode::Run);
        assert_eq!(control.hit(), None);
        assert!(!control.arrive(6, 999, &idle));
        assert!(control.arrive(7, 1000, &idle));
        control.set(ReplayMode::Run);
        assert!(
            !control.arrive(8, 2000, &idle),
            "each breakpoint pauses once"
        );
        assert_eq!(control.breakpoints(), []);
    }

    #[test]
    fn a_blocked_breakpoint_reads_the_scheduler_only_when_set() {
        let control = ReplayControl::default();
        let unread = || -> SchedulerView { panic!("read without a breakpoint needing it") };
        assert!(!control.arrive(1, 0, &unread));
        control.add_breakpoint(Breakpoint::Blocked {
            thread: "driver-1".into(),
            on: "Io".into(),
        });
        assert!(!control.arrive(2, 0, &|| blocked("driver-0", "Io(3)")));
        assert!(!control.arrive(3, 0, &|| blocked("driver-1", "Join(2)")));
        assert!(control.arrive(4, 0, &|| blocked("driver-1", "Io(3)")));
    }

    #[test]
    fn failures_pause_only_with_a_failure_breakpoint_which_stays() {
        let control = ReplayControl::default();
        assert!(!control.failure("boom"));
        control.add_breakpoint(Breakpoint::Failure);
        assert!(control.failure("boom"));
        assert_eq!(control.hit().as_deref(), Some("failure: boom"));
        control.set(ReplayMode::Run);
        assert!(control.failure("again"));
        assert_eq!(control.breakpoints(), [Breakpoint::Failure]);
    }
}
