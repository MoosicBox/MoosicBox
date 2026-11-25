//! Steering a simulation while it runs: pausing and resizing the fleet of workers, and
//! starting replay and shrink jobs beside it.
//!
//! The fleet is the simulation's own runs (`SIMULATOR_RUNS` of them, or unbounded with
//! `SIMULATOR_RUNS=forever`), made by up to `SIMULATOR_MAX_PARALLEL` worker threads. Jobs are
//! extra runs an observer asks for: each runs on a thread of its own, never counts toward the
//! fleet's totals, and never stops the fleet.

use std::{
    collections::BTreeSet,
    sync::{
        Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use simvar_observer::{ReplayControl, RunId};

/// What the fleet does when one of its runs fails: `SIMULATOR_ON_FAILURE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnFailure {
    /// End the whole simulation (the default): no new runs start and running ones are
    /// cancelled.
    Stop,
    /// Record and report the failure, and keep starting runs.
    Continue,
}

impl OnFailure {
    /// The policy from `SIMULATOR_ON_FAILURE`: `stop` (the default) or `continue`.
    ///
    /// # Panics
    ///
    /// * If `SIMULATOR_ON_FAILURE` is set to anything else
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("SIMULATOR_ON_FAILURE").as_deref() {
            Err(_) | Ok("" | "stop") => Self::Stop,
            Ok("continue") => Self::Continue,
            Ok(other) => panic!("SIMULATOR_ON_FAILURE must be stop or continue, got '{other}'"),
        }
    }
}

/// A snapshot of the fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FleetStatus {
    /// Whether workers are held from starting new runs.
    pub paused: bool,
    /// How many workers the fleet runs at most.
    pub max_parallel: u64,
    /// Fleet runs in progress.
    pub active: u64,
    /// Fleet runs finished, passed or failed.
    pub completed: u64,
    /// Fleet runs that failed (included in `completed`).
    pub failed: u64,
}

/// An in-process replay of an earlier run, for [`FleetControl::replay`].
#[derive(Debug, Clone)]
pub struct ReplayRequest {
    /// The run's seed.
    pub seed: u64,
    /// The run's simulated clock's epoch offset, as its `RunStarted` event reported it.
    /// Ignored without the `time` feature.
    pub epoch_offset: u64,
    /// Holds the replay at scheduling decisions (see [`crate::current_replay`]); `None` runs
    /// it freely.
    pub control: Option<Arc<ReplayControl>>,
}

/// Why a job could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JobError {
    /// No simulation has started in this process, so there is nothing to replay yet.
    #[error("no simulation has started in this process")]
    NotStarted,
}

/// What a job thread is asked to run.
pub enum Job {
    Replay(ReplayRequest),
    Verify {
        seed: u64,
        epoch_offset: u64,
    },
    #[cfg(feature = "thread")]
    Shrink {
        seed: u64,
        epoch_offset: u64,
    },
}

/// Runs jobs with the simulation's bootstrap; installed by `run_simulation`.
pub trait JobRunner: Send + Sync {
    /// Run `job` as job number `number`, on the calling thread.
    fn run(&self, number: u64, job: Job);
}

#[derive(Debug, Default)]
pub struct FleetState {
    pub paused: bool,
    pub max_parallel: u64,
    pub active: u64,
    pub completed: u64,
    pub failed: u64,
    /// Worker threads alive.
    pub workers: u64,
    /// Worker slots in use; a new worker takes the lowest free one.
    pub slots: BTreeSet<u64>,
    /// Whether the fleet's workers are running (between start and the last worker's exit).
    pub running: bool,
    /// Whether someone stopped the simulation (rather than a failure ending it).
    pub stopped: bool,
}

/// Steers the fleet and starts jobs; see [`control`].
pub struct FleetControl {
    state: Mutex<FleetState>,
    changed: Condvar,
    runner: Mutex<Option<Arc<dyn JobRunner>>>,
    next_job: AtomicU64,
}

impl std::fmt::Debug for FleetControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FleetControl")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

static CONTROL: LazyLock<Arc<FleetControl>> = LazyLock::new(|| {
    Arc::new(FleetControl {
        state: Mutex::new(FleetState::default()),
        changed: Condvar::new(),
        runner: Mutex::new(None),
        next_job: AtomicU64::new(1),
    })
});

/// The process's fleet control. Usable from any thread at any time; settings made before the
/// simulation starts apply to it (except `max_parallel`, which `SIMULATOR_MAX_PARALLEL` sets
/// at start).
#[must_use]
pub fn control() -> Arc<FleetControl> {
    Arc::clone(&CONTROL)
}

impl FleetControl {
    pub fn lock(&self) -> MutexGuard<'_, FleetState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn wait<'a>(&self, guard: MutexGuard<'a, FleetState>) -> MutexGuard<'a, FleetState> {
        self.changed
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Wake every worker and the orchestrator to look at the state again.
    pub fn notify(&self) {
        self.changed.notify_all();
    }

    pub fn install(&self, runner: Arc<dyn JobRunner>) {
        *self.runner.lock().unwrap_or_else(PoisonError::into_inner) = Some(runner);
    }

    /// Hold workers from starting new fleet runs; runs in progress finish. Jobs are not
    /// affected.
    pub fn pause(&self) {
        self.lock().paused = true;
        self.notify();
    }

    /// Let workers start fleet runs again.
    pub fn resume(&self) {
        self.lock().paused = false;
        self.notify();
    }

    /// Run at most `max_parallel` fleet workers. Growing starts more workers at once;
    /// shrinking retires workers as they finish their runs. `0` retires every worker until
    /// it is raised again. Has no effect when the simulation was started with
    /// `SIMULATOR_MAX_PARALLEL=0` (every run on the calling thread).
    pub fn set_max_parallel(&self, max_parallel: u64) {
        self.lock().max_parallel = max_parallel;
        self.notify();
    }

    /// End the simulation, as [`crate::end_sim`] does: no new runs start and running ones
    /// (jobs included) are cancelled.
    pub fn stop(&self) {
        crate::end_sim();
    }

    /// The fleet as it is now.
    #[must_use]
    pub fn status(&self) -> FleetStatus {
        let state = self.lock();
        FleetStatus {
            paused: state.paused,
            max_parallel: state.max_parallel,
            active: state.active,
            completed: state.completed,
            failed: state.failed,
        }
    }

    pub fn mark_stopped(&self) {
        self.lock().stopped = true;
        self.notify();
    }

    /// Re-run the run that `request.seed` and `request.epoch_offset` produced, exactly, on a
    /// thread of its own (kind [`simvar_observer::RunKind::Replay`]), with the usual events.
    /// Works while the fleet runs, while it is paused and after it finished; several replays
    /// may run at once. A replay never counts toward the fleet and never stops it.
    ///
    /// # Errors
    ///
    /// * [`JobError::NotStarted`] if no simulation has started in this process
    pub fn replay(&self, request: ReplayRequest) -> Result<RunId, JobError> {
        self.spawn_job(Job::Replay(request))
    }

    /// Check that the run `seed` and `epoch_offset` produced is deterministic, on a thread of
    /// its own: run it (kind [`simvar_observer::RunKind::Replay`], with the usual events),
    /// recording everything it observes, then replay it alone in a fresh, perturbed process of
    /// this program, as `SIMULATOR_VERIFY_DETERMINISM=process` does, and report what was found
    /// as a [`simvar_observer::SimEvent::Verified`]: where the two runs part, if they do.
    ///
    /// # Errors
    ///
    /// * [`JobError::NotStarted`] if no simulation has started in this process
    pub fn verify(&self, seed: u64, epoch_offset: u64) -> Result<RunId, JobError> {
        self.spawn_job(Job::Verify { seed, epoch_offset })
    }

    /// Shrink the thread schedule of the run that `seed` and `epoch_offset` produced, on a
    /// thread of its own: the run itself first (kind [`simvar_observer::RunKind::Shrink`],
    /// with the usual events), then silent replays reported as
    /// [`simvar_observer::SimEvent::Shrink`] progress, up to `SIMULATOR_SHRINK` replays (1000
    /// when it is not a number). The last `Shrink` event (`done`) carries the smallest
    /// failing schedule for `SIMULATOR_SCHEDULE`.
    ///
    /// # Errors
    ///
    /// * [`JobError::NotStarted`] if no simulation has started in this process
    #[cfg(feature = "thread")]
    pub fn shrink(&self, seed: u64, epoch_offset: u64) -> Result<RunId, JobError> {
        self.spawn_job(Job::Shrink { seed, epoch_offset })
    }

    fn spawn_job(&self, job: Job) -> Result<RunId, JobError> {
        let runner = self
            .runner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(JobError::NotStarted)?;
        let number = self.next_job.fetch_add(1, Ordering::SeqCst);
        // A simulation that a failure ended (`SIMULATOR_ON_FAILURE=stop`) and whose workers
        // are all done leaves every run cancelled; a job started afterwards must still run.
        {
            let state = self.lock();
            if !state.running && !state.stopped && simvar_utils::is_global_simulator_cancelled() {
                simvar_utils::reset_global_simulator_cancellation_token();
            }
        }
        std::thread::Builder::new()
            .name(format!("simvar-job-{number}"))
            .spawn(move || runner.run(number, job))
            .expect("failed to spawn a simvar job thread");
        Ok(RunId {
            worker: 0,
            run: number,
        })
    }
}
