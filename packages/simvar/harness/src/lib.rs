//! Deterministic simulation test harness for concurrent systems.
//!
//! This crate provides a framework for running deterministic simulations of distributed
//! systems and concurrent applications. It allows you to test complex scenarios involving
//! multiple actors (hosts and clients) with controlled timing, randomness, and networking.
//!
//! # Features
//!
//! * **Deterministic execution** - Same seed produces identical simulation results
//! * **Host and client actors** - Model persistent services (hosts) and ephemeral clients
//! * **Fault injection** - Crash, restart and bounce hosts, and partition or degrade the
//!   simulated network ([`fault`])
//! * **Simulation lifecycle hooks** - Customize behavior at key points via [`SimBootstrap`]
//! * **Built-in TUI** - Optional terminal UI for monitoring simulation progress
//! * **Parallel execution** - Run multiple simulation runs concurrently
//! * **Cancellation support** - Graceful shutdown with Ctrl-C handling
//!
//! # Example
//!
//! ```rust,no_run
//! # use simvar_harness::{run_simulation, SimBootstrap, Sim, SimConfig};
//! # use simvar_harness::host::HostResult;
//! # use simvar_harness::client::ClientResult;
//! struct MyBootstrap;
//!
//! impl SimBootstrap for MyBootstrap {
//!     fn build_sim(&self, config: SimConfig) -> SimConfig {
//!         config
//!     }
//!
//!     fn on_start(&self, sim: &mut impl Sim) {
//!         // Spawn a host actor
//!         sim.host("server", || async {
//!             // Server logic here
//!             Ok(())
//!         });
//!
//!         // Spawn a client actor
//!         sim.client("client", async {
//!             // Client logic here
//!             Ok(())
//!         });
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let results = run_simulation(MyBootstrap)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Environment Variables
//!
//! * `SIMULATOR_RUNS` - Number of simulation runs to execute (default: 1), or `forever` to
//!   run until stopped (only failed runs' results are then kept)
//! * `SIMULATOR_MAX_PARALLEL` - Maximum parallel runs (default: number of CPUs)
//! * `SIMULATOR_ON_FAILURE` - `stop` (the default) ends the simulation at the first failed run;
//!   `continue` records and reports it and keeps starting runs
//! * `SIMULATOR_SEED` - Fixed random seed for deterministic runs
//! * `SIMULATOR_EPOCH_OFFSET` - Fixed epoch offset override (when `time` feature is enabled)
//! * `SIMULATOR_EPOCH_MIN` / `SIMULATOR_EPOCH_MAX` - Bounded random epoch offset (inclusive)
//! * `SIMULATOR_EPOCH_RANGE_PROFILE` - Epoch profile (`low`, `wide`, `full`)
//! * `SIMULATOR_VERIFY_DETERMINISM` - Replay every run and fail if its trace differs
//! * `SIMULATOR_SHRINK` - With the `thread` feature: shrink a failing run's thread schedule
//!   (`1`/`true` for up to 1000 replays, or a number of replays) and print how to replay it
//! * `NO_TUI` - Disable terminal UI when set (at build or run time); it is also off when stdout is not a terminal
//!
//! # Steering a running simulation
//!
//! [`control`] pauses, resumes, resizes and stops the fleet of runs, and starts replay and
//! shrink jobs beside it. Inside a run, [`current_run`] and [`current_replay`] say which run
//! the calling thread belongs to and which [`observer::ReplayControl`] (if any) holds it.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    panic::AssertUnwindSafe,
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use client::{Client, ClientResult};
use color_backtrace::{BacktracePrinter, termcolor::Buffer};
use config::run_info;
use formatting::TimeFormat as _;
use host::{Host, HostResult};
use simvar_observer::{ReplayControl, RunId, RunKind, SimEvent};
use simvar_utils::{
    cancel_global_simulation, cancel_simulation, is_global_simulator_cancelled,
    is_simulator_cancelled, reset_simulator_cancellation_token, worker_thread_id,
};
use switchy::{
    random::{rand::rand::seq::SliceRandom as _, rng},
    time::simulator::{current_step, next_step, reset_step},
    unsync::thread_id,
};

pub use config::{SimConfig, SimProperties, SimResult, SimRunProperties};
pub use fleet::{FleetControl, FleetStatus, JobError, OnFailure, ReplayRequest, control};
pub use logging::set_quiet;
pub use simvar_observer as observer;
pub use simvar_utils as utils;

pub use switchy;

/// Client actor types and utilities.
///
/// Provides the [`Client`] type for modeling ephemeral actors in simulations.
///
/// [`Client`]: client::Client
pub mod client;

mod config;
mod fleet;

/// Time formatting utilities.
///
/// Provides the [`TimeFormat`] trait for converting time durations
/// in milliseconds into human-readable formatted strings.
///
/// [`TimeFormat`]: formatting::TimeFormat
pub mod formatting;

/// Fault injection: crash, restart and bounce hosts, and break the simulated network.
pub mod fault;

/// Host actor types and utilities.
///
/// Provides the [`Host`] type for modeling persistent actors that can be restarted.
///
/// [`Host`]: host::Host
pub mod host;

mod logging;
/// Interaction planning utilities.
///
/// Provides the [`InteractionPlan`] trait for managing sequences of planned interactions.
///
/// [`InteractionPlan`]: plan::InteractionPlan
pub mod plan;

/// Whether the TUI is shown: the `tui` feature is enabled, `NO_TUI` is set neither at build
/// time nor at run time, and standard output is a terminal.
static USE_TUI: LazyLock<bool> = LazyLock::new(|| {
    use std::io::IsTerminal as _;
    cfg!(feature = "tui")
        && std::option_env!("NO_TUI").is_none()
        && std::env::var_os("NO_TUI").is_none()
        && std::io::stdout().is_terminal()
});

thread_local! {
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// How many fleet runs to make: `SIMULATOR_RUNS`, `None` for `forever`.
static RUNS: LazyLock<Option<u64>> = LazyLock::new(|| {
    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(Some(1), |x| parse_runs(&x))
});

/// `SIMULATOR_RUNS`'s value: a number of runs, or `None` for `forever`.
fn parse_runs(value: &str) -> Option<u64> {
    if value == "forever" {
        return None;
    }
    Some(value.parse::<u64>().unwrap_or_else(|_| {
        panic!("SIMULATOR_RUNS must be a number of runs or 'forever', got '{value}'")
    }))
}

static ON_FAILURE: LazyLock<OnFailure> = LazyLock::new(OnFailure::from_env);

/// Whether a failed fleet run ends the simulation.
fn stop_on_failure() -> bool {
    *ON_FAILURE == OnFailure::Stop
}

static END_SIM: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

/// simvar's terminal display, while it is shown.
#[cfg(feature = "tui")]
static DISPLAY: Mutex<Option<Arc<simvar_ratatui::Display>>> = Mutex::new(None);

/// Errors that can occur during simulation execution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O operation failed.
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Simulation step returned an error.
    #[error(transparent)]
    Step(Box<dyn std::error::Error + Send>),
    /// Task join operation failed.
    #[error(transparent)]
    Join(#[from] switchy::unsync::task::JoinError),
}

fn ctrl_c() {
    log::debug!("ctrl_c called");
    #[cfg(feature = "tui")]
    if let Some(display) = &*DISPLAY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        display.exit();
    }
    end_sim();
}

/// Signals all running simulations to stop.
///
/// This sets a global flag and cancels the global simulation, causing all
/// simulation runs (replay and shrink jobs included) to terminate gracefully.
pub fn end_sim() {
    control().mark_stopped();
    end_fleet();
}

/// Start no more fleet runs and cancel every run in progress, for a failure
/// (`SIMULATOR_ON_FAILURE=stop`) or a stop.
fn end_fleet() {
    END_SIM.store(true, Ordering::SeqCst);

    if !is_global_simulator_cancelled() {
        cancel_global_simulation();
    }
    control().notify();
}

fn try_get_backtrace() -> Option<String> {
    let bt = std::backtrace::Backtrace::force_capture();
    let bt = btparse::deserialize(&bt).ok()?;

    let mut buffer = Buffer::ansi();
    BacktracePrinter::default()
        .print_trace(&bt, &mut buffer)
        .ok()?;

    Some(String::from_utf8_lossy(buffer.as_slice()).to_string())
}

/// Executes one or more simulation runs using the provided bootstrap implementation.
///
/// This is the main entry point for running simulations. It sets up the environment,
/// handles parallel execution if configured, and returns the results of all runs.
/// The number of runs and parallelism level can be controlled via the `SIMULATOR_RUNS`
/// and `SIMULATOR_MAX_PARALLEL` environment variables.
///
/// # Panics
///
/// * If `SIMULATOR_RUNS` or `SIMULATOR_MAX_PARALLEL` is set but cannot be parsed
///   as a `u64`.
/// * If the Ctrl-C handler cannot be installed.
/// * If converting the available parallelism value to `u64` fails unexpectedly.
/// * If TUI shutdown thread joining fails when the `tui` feature is enabled.
/// * If elapsed wall-clock time cannot be measured because system time goes
///   backwards during a run.
///
/// # Errors
///
/// * The contents of this function are wrapped in a `catch_unwind` call, so if
///   any panic happens, it will be wrapped into an error on the outer `Result`
/// * If the `Sim` `step` returns an error, we return that in an Ok(Err(e))
/// * If simulation worker threads report orchestration errors
/// * If logger initialization fails when `pretty_env_logger` is enabled
///
/// # Examples
///
/// ```rust,no_run
/// use simvar_harness::{Sim, SimBootstrap, SimConfig, run_simulation};
///
/// struct Bootstrap;
///
/// impl SimBootstrap for Bootstrap {
///     fn build_sim(&self, config: SimConfig) -> SimConfig {
///         config
///     }
///
///     fn on_start(&self, sim: &mut impl Sim) {
///         sim.client("client", async { Ok(()) });
///     }
/// }
///
/// let _results = run_simulation(Bootstrap)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[allow(clippy::let_and_return)]
pub fn run_simulation<B: SimBootstrap>(
    bootstrap: B,
) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
    static MAX_PARALLEL: LazyLock<u64> = LazyLock::new(|| {
        std::env::var("SIMULATOR_MAX_PARALLEL").ok().map_or_else(
            || {
                u64::try_from(std::thread::available_parallelism().map_or(1usize, Into::into))
                    .unwrap()
            },
            |x| x.parse::<u64>().unwrap(),
        )
    });

    // claim thread_id 1 for main thread
    let _ = thread_id();

    ctrlc::set_handler(ctrl_c).expect("Error setting Ctrl-C handler");

    #[cfg(feature = "pretty_env_logger")]
    logging::init_pretty_env_logger()?;

    // The display is an observer like any other: it learns everything from run events.
    #[cfg(feature = "tui")]
    let display = if *USE_TUI {
        let display = simvar_ratatui::spawn(end_sim)?;
        add_observer(display.clone());
        *DISPLAY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&display));
        Some(display)
    } else {
        None
    };

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new({
        move |x| {
            let thread_id = thread_id();
            let mut panic_str = x.to_string();
            if let Some(bt) = try_get_backtrace() {
                panic_str = format!("{panic_str}\n{bt}");
            }
            log::debug!("caught panic on thread_id={thread_id}: {panic_str}");
            PANIC.with_borrow_mut(|x| *x = Some(panic_str.clone()));
            // A panic ends its run; one in a fleet run ends the whole simulation unless the
            // fleet continues after failures.
            let run = current_run_state();
            match run {
                Some(run) if run.kind != RunKind::Fleet || !stop_on_failure() => {
                    run.panicked(panic_str);
                }
                Some(_) => end_fleet(),
                // Outside every run (an observer, or the program driving the simulation): no
                // run reports it, so report it as it would have been.
                None => {
                    end_fleet();
                    previous_hook(x);
                }
            }
        }
    }));

    let runs = *RUNS;
    let max_parallel = *MAX_PARALLEL;

    log::debug!("Running simulation with max_parallel={max_parallel}");

    notify(&SimEvent::Started { runs, max_parallel });
    let sim_orchestrator = SimOrchestrator::new(bootstrap, runs, max_parallel);

    let resp = sim_orchestrator.start();
    notify(&SimEvent::Finished);

    // The display stays up once every run is done, until the user quits it.
    #[cfg(feature = "tui")]
    if let Some(display) = display {
        display.join()?;
        *DISPLAY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    #[cfg(feature = "tui")]
    if *USE_TUI && let Ok(results) = &resp {
        eprintln!(
            "{}",
            results
                .iter()
                .filter(|x| !x.is_success())
                .map(SimResult::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    resp
}

struct SimOrchestrator<B: SimBootstrap> {
    bootstrap: B,
    runs: Option<u64>,
    max_parallel: u64,
}

/// Runs replay and shrink jobs with the simulation's bootstrap.
struct Jobs<B: SimBootstrap>(Arc<B>);

impl<B: SimBootstrap> fleet::JobRunner for Jobs<B> {
    fn run(&self, number: u64, job: fleet::Job) {
        let _ = thread_id();
        let simulation = Simulation::new(&*self.0);
        match job {
            fleet::Job::Replay(request) => {
                let spec = RunSpec {
                    kind: RunKind::Replay,
                    worker: 0,
                    epoch_offset: Some(request.epoch_offset),
                    control: request.control,
                };
                let _ = simulation.run_once(number, None, Replay::Seed(request.seed), &spec);
            }
            #[cfg(feature = "thread")]
            fleet::Job::Shrink { seed, epoch_offset } => {
                simulation.shrink_job(number, seed, epoch_offset);
            }
        }
    }
}

/// Bookkeeping for a fleet worker thread, undone when it exits (also by a panic).
struct WorkerSlot {
    control: Arc<FleetControl>,
    slot: u64,
    /// Whether the worker already gave its slot back (see [`Self::release`]).
    released: bool,
}

impl WorkerSlot {
    /// Give the slot back under the fleet's lock, so workers deciding whether to retire at
    /// the same time count this one as gone.
    fn release(&mut self, state: &mut fleet::FleetState) {
        if !self.released {
            self.released = true;
            state.workers -= 1;
            state.slots.remove(&self.slot);
        }
    }
}

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        let control = Arc::clone(&self.control);
        self.release(&mut control.lock());
        control.notify();
    }
}

/// Every fleet run's result, in run order (only failures when the fleet is unbounded).
type Results = Arc<Mutex<BTreeMap<u64, SimResult>>>;

impl<B: SimBootstrap> SimOrchestrator<B> {
    const fn new(bootstrap: B, runs: Option<u64>, max_parallel: u64) -> Self {
        Self {
            bootstrap,
            runs,
            max_parallel,
        }
    }

    fn start(self) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
        let control = control();
        let bootstrap = Arc::new(self.bootstrap);
        let results: Results = Arc::new(Mutex::new(BTreeMap::new()));
        let runs = self.runs;

        control.install(Arc::new(Jobs(Arc::clone(&bootstrap))));
        {
            let mut state = control.lock();
            state.max_parallel = self.max_parallel;
            state.running = true;
        }

        let outcome = if self.max_parallel == 0 {
            Self::run_sequentially(&control, &bootstrap, runs, &results);
            Ok(())
        } else {
            Self::run_workers(&control, &bootstrap, runs, &results)
        };
        control.lock().running = false;
        outcome?;

        Ok(Arc::try_unwrap(results)
            .unwrap()
            .into_inner()
            .unwrap()
            .into_values()
            .collect())
    }

    /// Every run on the calling thread (`SIMULATOR_MAX_PARALLEL=0`).
    fn run_sequentially(
        control: &FleetControl,
        bootstrap: &B,
        runs: Option<u64>,
        results: &Results,
    ) {
        let simulation = Simulation::new(bootstrap);
        for run_number in 1.. {
            if runs.is_some_and(|runs| run_number > runs) {
                break;
            }
            {
                let mut state = control.lock();
                while state.paused && !END_SIM.load(Ordering::SeqCst) {
                    state = control.wait(state);
                }
                if END_SIM.load(Ordering::SeqCst) {
                    break;
                }
                state.active += 1;
            }

            let result = simulation.run(run_number, None, 1);
            finish_run(control, runs, results, run_number, result);

            if END_SIM.load(Ordering::SeqCst) {
                break;
            }
        }
    }

    /// Runs on up to `max_parallel` worker threads, started and retired as the fleet control
    /// says, until every run is done or the simulation ends.
    fn run_workers(
        control: &Arc<FleetControl>,
        bootstrap: &Arc<B>,
        runs: Option<u64>,
        results: &Results,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let run_index = Arc::new(AtomicU64::new(0));
        let mut threads = vec![];

        let mut state = control.lock();
        loop {
            let ended = END_SIM.load(Ordering::SeqCst);
            // Runs no worker has taken yet, beyond those the idle workers will take.
            let unclaimed = runs.map(|runs| runs.saturating_sub(run_index.load(Ordering::SeqCst)));
            while !ended
                && state.workers < state.max_parallel
                && unclaimed.is_none_or(|unclaimed| state.workers < unclaimed)
            {
                // One of the first `slots + 1` numbers is always free.
                let slot = (1..=state.slots.len() as u64 + 1)
                    .find(|slot| !state.slots.contains(slot))
                    .expect("a free worker slot");
                state.slots.insert(slot);
                state.workers += 1;
                log::debug!("starting worker {slot}");
                threads.push(Self::spawn_worker(
                    WorkerSlot {
                        control: Arc::clone(control),
                        slot,
                        released: false,
                    },
                    Arc::clone(bootstrap),
                    Arc::clone(&run_index),
                    runs,
                    Arc::clone(results),
                ));
            }
            if state.workers == 0 && (ended || unclaimed == Some(0)) {
                break;
            }
            state = control.wait(state);
        }
        drop(state);

        let mut errors = vec![];

        for (i, thread) in threads.into_iter().enumerate() {
            log::debug!("joining thread {i}...");

            match thread.join() {
                Ok(x) => {
                    if let Err(e) = x {
                        errors.push(e);
                    }
                    log::debug!("thread {i} joined");
                }
                Err(e) => {
                    let message = e
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("(not a string)");
                    log::error!("failed to join thread {i}: {message}");
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("\n").into());
        }
        Ok(())
    }

    fn spawn_worker(
        worker: WorkerSlot,
        bootstrap: Arc<B>,
        run_index: Arc<AtomicU64>,
        runs: Option<u64>,
        results: Results,
    ) -> std::thread::JoinHandle<Result<(), String>> {
        std::thread::spawn(move || {
            let mut worker = worker;
            let _ = thread_id();
            let thread_id = worker_thread_id();
            let slot = worker.slot;
            let control = Arc::clone(&worker.control);
            let simulation = Simulation::new(&*bootstrap);

            loop {
                let run_index = {
                    let mut state = control.lock();
                    loop {
                        if END_SIM.load(Ordering::SeqCst) {
                            log::debug!(
                                "simulation has ended. worker {slot} ({thread_id}) finished"
                            );
                            return Ok(());
                        }
                        if state.workers > state.max_parallel {
                            log::debug!("worker {slot} ({thread_id}) retired");
                            worker.release(&mut state);
                            return Ok(());
                        }
                        if !state.paused {
                            break;
                        }
                        state = control.wait(state);
                    }
                    let run_index = run_index.fetch_add(1, Ordering::SeqCst);
                    if runs.is_some_and(|runs| run_index >= runs) {
                        log::debug!(
                            "finished all runs ({runs:?}). worker {slot} ({thread_id}) finished"
                        );
                        return Ok(());
                    }
                    state.active += 1;
                    run_index
                };

                log::debug!(
                    "starting simulation run_index={run_index} on worker {slot} ({thread_id})"
                );

                let result = simulation.run(run_index + 1, Some(thread_id), slot);
                finish_run(&control, runs, &results, run_index + 1, result);

                log::debug!(
                    "simulation finished run_index={run_index} on worker {slot} ({thread_id})"
                );
            }
        })
    }
}

/// Count a finished fleet run and keep its result (only a failure's, when the fleet is
/// unbounded, so memory stays flat).
fn finish_run(
    control: &FleetControl,
    runs: Option<u64>,
    results: &Results,
    run_number: u64,
    result: SimResult,
) {
    {
        let mut state = control.lock();
        state.active -= 1;
        state.completed += 1;
        if !result.is_success() {
            state.failed += 1;
        }
    }
    control.notify();
    if runs.is_some() || !result.is_success() {
        results
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_number, result);
    }
}

/// How [`Simulation::run_once`] picks its seed and schedule.
#[cfg_attr(not(feature = "thread"), derive(Clone, Copy))]
enum Replay {
    /// The next seed in the sequence.
    No,
    /// An earlier run's seed.
    Seed(u64),
    /// An earlier run's seed with this thread schedule, reporting nothing.
    #[cfg(feature = "thread")]
    Silent(u64, switchy::thread::simulator::Schedule),
}

/// What one run produced.
struct RunOutcome {
    result: SimResult,
    /// The run's trace hash.
    trace: u64,
    seed: u64,
    /// The thread schedule it followed.
    #[cfg(feature = "thread")]
    schedule: switchy::thread::simulator::Schedule,
}

/// How one run is made: why, by which worker, and under which control.
struct RunSpec {
    kind: RunKind,
    /// The fleet worker slot (`0` for a job).
    worker: u64,
    /// Pin the clock's epoch offset to this (a replay of a recorded run).
    #[cfg_attr(not(feature = "time"), allow(dead_code))]
    epoch_offset: Option<u64>,
    control: Option<Arc<ReplayControl>>,
}

impl RunSpec {
    const fn fleet(worker: u64) -> Self {
        Self {
            kind: RunKind::Fleet,
            worker,
            epoch_offset: None,
            control: None,
        }
    }

    /// The run's id: fleet runs carry their worker thread, jobs worker `0`.
    fn id(&self, run_number: u64, thread_id: Option<u64>) -> RunId {
        RunId {
            worker: match self.kind {
                RunKind::Fleet => thread_id.unwrap_or(1),
                RunKind::Replay | RunKind::Shrink => 0,
            },
            run: run_number,
        }
    }
}

/// What the current simulation context knows about the run it belongs to.
struct RunState {
    id: RunId,
    kind: RunKind,
    control: Option<Arc<ReplayControl>>,
    /// The first panic on any of the run's threads (simulated ones included).
    panic: Mutex<Option<String>>,
    /// Set by a panic that ends only this run.
    cancelled: AtomicBool,
}

impl RunState {
    fn panicked(&self, panic: String) {
        self.panic
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(panic);
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

/// The run the current simulation context belongs to.
struct CurrentRun(Option<Arc<RunState>>);

fn current_run_state() -> Option<Arc<RunState>> {
    switchy::simulation::state_with(|| CurrentRun(None))
        .0
        .clone()
}

/// The run the calling thread belongs to (its worker thread or one of its simulated threads),
/// or `None` outside every run.
#[must_use]
pub fn current_run() -> Option<RunId> {
    current_run_state().map(|run| run.id)
}

/// The [`ReplayControl`] holding the current run at scheduling decisions, if it has one.
///
/// Only a replay job started with one has it; fleet runs have none. A simulation's decision
/// hook uses it to hold each run under its own control.
#[must_use]
pub fn current_replay() -> Option<Arc<ReplayControl>> {
    current_run_state().and_then(|run| run.control.clone())
}

/// Marks a run whose reports, events and telemetry are dropped (a shrinking replay).
struct Silent(bool);

/// Whether the current run reports nothing.
fn silent() -> bool {
    switchy::simulation::state_with(|| Silent(false)).0
}

/// How many replays `SIMULATOR_SHRINK` allows shrinking a failure: `1` or `true` for
/// [`switchy::thread::simulator::SHRINK_ATTEMPTS`], or a number of replays.
#[cfg(feature = "thread")]
fn shrink_attempts() -> Option<usize> {
    let value = std::env::var("SIMULATOR_SHRINK").ok()?;
    match value.as_str() {
        "" | "0" | "false" => None,
        "1" | "true" => Some(switchy::thread::simulator::SHRINK_ATTEMPTS),
        other => Some(other.parse().unwrap_or_else(|_| {
            panic!("SIMULATOR_SHRINK must be 1, true or a number of replays, got '{other}'")
        })),
    }
}

struct Simulation<'a, B: SimBootstrap> {
    bootstrap: &'a B,
}

impl<'a, B: SimBootstrap> Simulation<'a, B> {
    const fn new(bootstrap: &'a B) -> Self {
        Self { bootstrap }
    }

    /// Run fleet run `run_number` on worker slot `worker` once (shrinking a failure's schedule
    /// if asked to), and with determinism verification on, run the same seed again and fail
    /// if the two runs' traces differ. A failure ends the simulation, unless the fleet
    /// continues after failures.
    fn run(&self, run_number: u64, thread_id: Option<u64>, worker: u64) -> SimResult {
        let result = self.run_checked(run_number, thread_id, &RunSpec::fleet(worker));
        if !result.is_success() && stop_on_failure() {
            end_fleet();
        }
        result
    }

    fn run_checked(&self, run_number: u64, thread_id: Option<u64>, spec: &RunSpec) -> SimResult {
        let first = self.run_once(run_number, thread_id, Replay::No, spec);
        if !first.result.is_success() {
            return self.shrunk(run_number, thread_id, first, spec);
        }
        if !verify_determinism() {
            return first.result;
        }
        let again = self.run_once(run_number, thread_id, Replay::Seed(first.seed), spec);
        if !again.result.is_success() {
            return again.result;
        }
        let (result, trace, seed, replayed) = (first.result, first.trace, first.seed, again.trace);
        if replayed == trace {
            return result;
        }
        let SimResult::Success { props, run } = result else {
            unreachable!("checked above")
        };
        let result = SimResult::Fail {
            props,
            run,
            error: Some(format!(
                "non-deterministic simulation: replaying seed {seed} produced trace {replayed:#018x}, \
                 not {trace:#018x}"
            )),
            panic: None,
        };
        logging::log_message(result.to_string());
        result
    }

    /// With `SIMULATOR_SHRINK` (and the `thread` feature), shrink the failed run's thread
    /// schedule by replaying its seed, and report the smallest failing run with how to replay
    /// it. Otherwise, the failed run's result.
    #[cfg_attr(not(feature = "thread"), allow(clippy::unused_self, unused_variables))]
    fn shrunk(
        &self,
        run_number: u64,
        thread_id: Option<u64>,
        failed: RunOutcome,
        spec: &RunSpec,
    ) -> SimResult {
        #[cfg(feature = "thread")]
        if let Some(attempts) = shrink_attempts() {
            let seed = failed.seed;
            let original = failed.schedule.choices.len();
            let schedule = self.shrink_schedule(
                run_number,
                thread_id,
                seed,
                &failed.schedule,
                attempts,
                spec,
                |_, _| {},
            );
            let last = self.run_once(
                run_number,
                thread_id,
                Replay::Silent(seed, schedule.clone()),
                spec,
            );
            if let SimResult::Fail {
                props,
                run,
                error,
                panic,
            } = last.result
            {
                #[cfg(feature = "time")]
                let epoch_offset = Some(props.config.epoch_offset);
                #[cfg(not(feature = "time"))]
                let epoch_offset = None;
                let run_env: Vec<(&str, &str)> = props
                    .extra
                    .iter()
                    .filter(|(name, _)| config::is_environment_name(name))
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                let command = config::get_run_command_with(
                    &[
                        "SIMULATOR_SEED",
                        "SIMULATOR_RUNS",
                        "SIMULATOR_MAX_PARALLEL",
                        "SIMULATOR_SHRINK",
                        "SIMULATOR_SCHEDULE",
                        "SIMULATOR_SCHEDULE_STRATEGY",
                    ],
                    seed,
                    epoch_offset,
                    &run_env,
                );
                let note = format!(
                    "shrunk the thread schedule from {original} choices to {}; replay with: \
                     SIMULATOR_RUNS=1 SIMULATOR_SCHEDULE={schedule} {command}",
                    schedule.choices.len()
                );
                let result = SimResult::Fail {
                    props,
                    run,
                    error: Some(error.map_or_else(|| note.clone(), |e| format!("{e}\n\n{note}"))),
                    panic,
                };
                logging::log_message(result.to_string());
                return result;
            }
        }
        failed.result
    }

    /// The smallest failing schedule [`switchy::thread::simulator::shrink`] finds for the
    /// failed run of `seed` that followed `schedule`, in at most `attempts` silent replays.
    /// `progress` hears each replay made and the smallest failing schedule's length so far.
    #[cfg(feature = "thread")]
    #[allow(clippy::too_many_arguments)]
    fn shrink_schedule(
        &self,
        run_number: u64,
        thread_id: Option<u64>,
        seed: u64,
        schedule: &switchy::thread::simulator::Schedule,
        attempts: usize,
        spec: &RunSpec,
        mut progress: impl FnMut(u64, usize),
    ) -> switchy::thread::simulator::Schedule {
        let mut tried = 0;
        let mut best = schedule.choices.len();
        switchy::thread::simulator::shrink(schedule, attempts, |candidate| {
            let outcome = self.run_once(
                run_number,
                thread_id,
                Replay::Silent(seed, candidate.clone()),
                spec,
            );
            tried += 1;
            let failed = (!outcome.result.is_success()).then_some(outcome.schedule);
            if let Some(followed) = &failed {
                best = best.min(followed.choices.len());
            }
            progress(tried, best);
            failed
        })
    }

    /// A shrink job: run the run of `seed` and `epoch_offset`, and if it fails, shrink its
    /// thread schedule, reporting progress as [`SimEvent::Shrink`].
    #[cfg(feature = "thread")]
    fn shrink_job(&self, number: u64, seed: u64, epoch_offset: u64) {
        let spec = RunSpec {
            kind: RunKind::Shrink,
            worker: 0,
            epoch_offset: Some(epoch_offset),
            control: None,
        };
        let id = spec.id(number, None);
        let first = self.run_once(number, None, Replay::Seed(seed), &spec);
        let original_len = first.schedule.choices.len() as u64;
        // Sent past the silent replays' suppression: these events are the job's report.
        let report = |attempts: u64, best_len: u64, schedule: Option<String>, done: bool| {
            notify_observers(&SimEvent::Shrink {
                id,
                attempts,
                best_len,
                original_len,
                done,
                schedule,
            });
        };
        if first.result.is_success() {
            report(0, original_len, None, true);
            return;
        }
        report(0, original_len, None, false);
        let attempts = shrink_attempts().unwrap_or(switchy::thread::simulator::SHRINK_ATTEMPTS);
        let mut tried = 0;
        let schedule = self.shrink_schedule(
            number,
            None,
            seed,
            &first.schedule,
            attempts,
            &spec,
            |attempts, best| {
                tried = attempts;
                report(attempts, best as u64, None, false);
            },
        );
        report(
            tried,
            schedule.choices.len() as u64,
            Some(schedule.to_string()),
            true,
        );
    }

    /// One run. `replay` repeats an earlier run's seed instead of advancing the seed sequence,
    /// and can fix its thread schedule (silently: no reports, events or telemetry).
    #[allow(clippy::too_many_lines)]
    fn run_once(
        &self,
        run_number: u64,
        thread_id: Option<u64>,
        replay: Replay,
        spec: &RunSpec,
    ) -> RunOutcome {
        // Each run starts from a fresh simulation context, so nothing of an earlier run on
        // this worker thread (simulated threads, clock, generator) carries over.
        switchy::simulation::set_current(None);
        // A panic recorded by an earlier run on this thread is not this run's.
        PANIC.with_borrow_mut(|panic| *panic = None);
        match replay {
            Replay::No => {
                if run_number > 1 {
                    switchy::random::simulator::reset_seed();
                }
            }
            Replay::Seed(seed) => switchy::simulation::set_seed(seed),
            #[cfg(feature = "thread")]
            Replay::Silent(seed, schedule) => {
                // The fleet failure being shrunk cancelled every run (and so may each failing
                // replay); this replay must still run. No new runs start either way (see
                // `END_SIM`).
                if spec.kind == RunKind::Fleet && stop_on_failure() {
                    simvar_utils::reset_global_simulator_cancellation_token();
                }
                switchy::simulation::set_seed(seed);
                let _ = switchy::simulation::state_with(|| Silent(true));
                let mut config = switchy::thread::simulator::Config::from_env(seed);
                config.strategy = switchy::thread::simulator::Strategy::Replay(Arc::new(schedule));
                switchy::thread::simulator::configure(config);
            }
        }
        let seed = switchy::random::simulator::seed();
        let run_id = spec.id(run_number, thread_id);
        let run_state = Arc::new(RunState {
            id: run_id,
            kind: spec.kind,
            control: spec.control.clone(),
            panic: Mutex::new(None),
            cancelled: AtomicBool::new(false),
        });
        let _ = switchy::simulation::state_with(|| CurrentRun(Some(Arc::clone(&run_state))));

        switchy::random::simulator::reset_rng();
        switchy::tcp::simulator::reset();
        switchy::network::reset();
        #[cfg(feature = "fs")]
        switchy::fs::simulator::reset_fs();
        #[cfg(feature = "time")]
        switchy::time::simulator::reset_epoch_offset();
        // A replay pins the recorded offset after drawing one, so every later draw matches.
        #[cfg(feature = "time")]
        if let Some(epoch_offset) = spec.epoch_offset {
            switchy::time::simulator::set_epoch_offset(epoch_offset);
        }
        #[cfg(feature = "time")]
        switchy::time::simulator::reset_step_multiplier();
        reset_simulator_cancellation_token();
        reset_step();

        self.bootstrap.init();

        let config = self.bootstrap.build_sim(SimConfig::from_rng());
        config.apply();
        let duration = config.duration;
        let duration_steps = duration.as_millis();

        let mut managed_sim = ManagedSim::new(config);

        let props = SimProperties {
            run_number,
            thread_id,
            config,
            extra: self.bootstrap.props(),
        };

        #[cfg(feature = "time")]
        let epoch_offset = config.epoch_offset;
        #[cfg(not(feature = "time"))]
        let epoch_offset = 0;
        notify(&SimEvent::RunStarted {
            id: run_id,
            seed,
            props: props.extra.clone(),
            steps: (duration < Duration::MAX)
                .then(|| u64::try_from(duration_steps).unwrap_or(u64::MAX)),
            worker: spec.worker,
            kind: spec.kind,
            epoch_offset,
        });

        logging::log_message(format!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(&props)
        ));

        let start = switchy::time::now();
        let real_start = Instant::now();

        self.bootstrap.on_start(&mut managed_sim);

        let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let print_step = |sim: &ManagedSim, step| {
                if duration < Duration::MAX {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = (step as f64 / duration_steps as f64).clamp(0.0, 1.0);

                    log::info!(
                        "step {step} ({}) ({:.1}%)",
                        sim.elapsed().as_millis().into_formatted(),
                        progress * 100.0,
                    );
                } else {
                    log::info!(
                        "step {step} ({})",
                        sim.elapsed().as_millis().into_formatted()
                    );
                }
            };

            managed_sim.start();

            loop {
                // A panic that ends only this run (see the panic hook) cancels it here.
                if run_state.cancelled.load(Ordering::SeqCst) && !is_simulator_cancelled() {
                    cancel_simulation();
                }
                if !is_simulator_cancelled() {
                    let step = next_step();

                    if duration < Duration::MAX && u128::from(step) >= duration_steps {
                        log::debug!("sim ran for {duration_steps} steps. stopping");
                        print_step(&managed_sim, step);
                        cancel_simulation();
                        break;
                    }

                    if step.is_multiple_of(1000) {
                        print_step(&managed_sim, step);
                    }
                    if step.is_multiple_of(100) {
                        #[allow(clippy::cast_precision_loss)]
                        notify(&SimEvent::Progress {
                            id: run_id,
                            step,
                            progress: (duration < Duration::MAX)
                                .then(|| (step as f64 / duration_steps as f64).clamp(0.0, 1.0)),
                        });
                    }

                    self.bootstrap.on_step(&mut managed_sim);
                }

                if managed_sim.step()? {
                    log::debug!("sim completed");
                    break;
                }
            }

            Ok::<_, Error>(())
        }));

        self.bootstrap.on_end(&mut managed_sim);

        let end = switchy::time::now();
        let real_time_millis = end.duration_since(start).unwrap().as_millis();
        let sim_time = managed_sim.elapsed();
        let sim_time_millis = sim_time.as_millis();
        // A run stopped before its first step ran none.
        let steps = current_step().saturating_sub(1);

        log::debug!("after simulation run");

        let run = SimRunProperties {
            steps,
            real_time_millis,
            sim_time_millis,
        };

        managed_sim.shutdown();

        let mut trace = run_trace();
        #[cfg(feature = "thread")]
        let (leaked_threads, schedule) = {
            let report = switchy::thread::simulator::scheduler().shutdown();
            trace = mix(trace, report.trace_hash);
            (report.leaked, report.schedule)
        };
        trace = mix(trace, steps);

        // A panic on this thread, or else on any other thread of the run.
        let panic = PANIC.with_borrow(Clone::clone).or_else(|| {
            run_state
                .panic
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        });

        let result = if let Err(e) = resp {
            SimResult::Fail {
                props,
                run,
                // A panic resumed (`resume_unwind`) reached no panic hook: its message, if any.
                error: if panic.is_none() {
                    Some(
                        e.downcast_ref::<String>()
                            .cloned()
                            .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                            .unwrap_or_else(|| format!("{e:?}")),
                    )
                } else {
                    None
                },
                panic,
            }
        } else if let Ok(Err(e)) = resp {
            SimResult::Fail {
                props,
                run,
                error: Some(e.to_string()),
                panic,
            }
        } else if let Some(panic) = panic {
            SimResult::Fail {
                props,
                run,
                error: None,
                panic: Some(panic),
            }
        } else {
            SimResult::Success { props, run }
        };
        #[cfg(feature = "thread")]
        let result = match result {
            SimResult::Success { props, run }
                if !leaked_threads.is_empty() && !allow_thread_leaks() =>
            {
                SimResult::Fail {
                    props,
                    run,
                    error: Some(format!(
                        "simulated threads never finished: {}",
                        leaked_threads.join(", ")
                    )),
                    panic: None,
                }
            }
            other => other,
        };

        if !result.is_success() && spec.kind == RunKind::Fleet && stop_on_failure() {
            // Start no more runs; the run's caller ends the simulation once it is done with
            // this failure (shrinking it, for example).
            END_SIM.store(true, Ordering::SeqCst);
        }

        logging::log_message(result.to_string());

        let trace = mix(trace, u64::from(result_is_success(&result)));
        notify(&SimEvent::RunFinished {
            id: run_id,
            seed,
            success: result_is_success(&result),
            report: result.to_string(),
            steps,
            real_time: real_start.elapsed(),
            sim_time,
        });
        RunOutcome {
            result,
            trace,
            seed,
            #[cfg(feature = "thread")]
            schedule,
        }
    }
}

const fn result_is_success(result: &SimResult) -> bool {
    matches!(result, SimResult::Success { .. })
}

static OBSERVERS: std::sync::RwLock<Vec<Arc<dyn simvar_observer::SimObserver>>> =
    std::sync::RwLock::new(Vec::new());

/// Send every simulation's events to `observer` from now on.
///
/// # Panics
///
/// * If the observer list's lock is poisoned
pub fn add_observer(observer: Arc<dyn simvar_observer::SimObserver>) {
    OBSERVERS.write().unwrap().push(observer);
}

fn notify(event: &SimEvent) {
    if silent() {
        return;
    }
    notify_observers(event);
}

/// Send `event` to every observer, also from a silent run.
fn notify_observers(event: &SimEvent) {
    let observers = OBSERVERS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for observer in observers.iter() {
        observer.event(event.clone());
    }
}

/// Send `payload` to every observer as telemetry of the current run.
///
/// Callable from any thread of the run, including simulated threads. Never changes the run.
/// Outside every run, the telemetry is sent as run `0` of worker `0`.
pub fn emit(payload: Arc<dyn std::any::Any + Send + Sync>) {
    let id = current_run().unwrap_or(RunId { worker: 0, run: 0 });
    notify(&SimEvent::Telemetry { id, payload });
}

/// Whether each run is replayed to check it is deterministic: `SIMULATOR_VERIFY_DETERMINISM`
/// set to `1` or `true`.
#[must_use]
pub fn verify_determinism() -> bool {
    std::env::var("SIMULATOR_VERIFY_DETERMINISM").is_ok_and(|v| v == "1" || v == "true")
}

/// Whether simulated threads still blocked when a run ends are tolerated:
/// `SIMULATOR_ALLOW_THREAD_LEAKS` set to `1` or `true`.
#[cfg(feature = "thread")]
fn allow_thread_leaks() -> bool {
    std::env::var("SIMULATOR_ALLOW_THREAD_LEAKS").is_ok_and(|v| v == "1" || v == "true")
}

/// The current run's trace of recorded values.
#[derive(Default)]
struct RunTrace(std::sync::Mutex<u64>);

fn mix(hash: u64, value: u64) -> u64 {
    value.to_le_bytes().iter().fold(hash, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Record an observable value of the current run (an output, an event, a final state) in its
/// trace. With `SIMULATOR_VERIFY_DETERMINISM`, a replay of the run must record the same values
/// in the same order.
///
/// # Panics
///
/// * If the trace's lock is poisoned
pub fn record(value: u64) {
    let trace = switchy::simulation::state::<RunTrace>();
    let mut hash = trace.0.lock().unwrap();
    *hash = mix(*hash, value);
}

fn run_trace() -> u64 {
    *switchy::simulation::state::<RunTrace>().0.lock().unwrap()
}

/// Trait for bootstrapping and configuring simulations.
///
/// Implement this trait to customize simulation behavior at various lifecycle
/// points. All methods have default implementations that do nothing.
pub trait SimBootstrap: Send + Sync + 'static {
    /// Returns custom properties to include in simulation output, asked once per run (with the
    /// run's seed current). A property named like an environment variable (`UPPER_CASE`; see
    /// [`config::is_environment_name`]) is one the run needs to replay, and goes in its replay
    /// command: a run chosen by more than its seed (by a guided fleet, say) reports its input so.
    #[must_use]
    fn props(&self) -> Vec<(String, String)> {
        vec![]
    }

    /// Modifies the simulation configuration before the simulation starts.
    #[must_use]
    fn build_sim(&self, config: SimConfig) -> SimConfig {
        config
    }

    /// Called once before any simulation runs begin.
    fn init(&self) {}

    /// Called when a simulation run starts.
    fn on_start(&self, #[allow(unused)] sim: &mut impl Sim) {}

    /// Called on each simulation step.
    fn on_step(&self, #[allow(unused)] sim: &mut impl Sim) {}

    /// Called when a simulation run ends.
    fn on_end(&self, #[allow(unused)] sim: &mut impl Sim) {}
}

/// Interface for managing simulation actors (hosts and clients) and injecting faults.
///
/// The fault methods have default implementations through [`fault`], whose functions can also
/// be called from inside actors (a nemesis client, for example).
pub trait Sim {
    /// Crash a host and restart it from its factory (see [`fault::bounce`]).
    fn bounce(&mut self, host: impl Into<String>);

    /// Crash a host and leave it down until restarted (see [`fault::crash`]).
    fn crash(&mut self, host: impl Into<String>) {
        fault::crash(host);
    }

    /// Restart a crashed host from its factory (see [`fault::restart`]).
    fn restart(&mut self, host: impl Into<String>) {
        fault::restart(host);
    }

    /// Crash a host and restart it after `delay` of simulated time (see
    /// [`fault::bounce_after`]).
    fn bounce_after(&mut self, host: impl Into<String>, delay: Duration) {
        fault::bounce_after(host, delay);
    }

    /// Run `hook` every time `host` crashes (see [`fault::on_crash`]).
    fn on_crash(&mut self, host: impl Into<String>, hook: impl FnMut(&str) + Send + 'static) {
        fault::on_crash(host, hook);
    }

    /// Partition the nodes of `a` from those of `b`, in both directions (see
    /// [`switchy::network::partition`]).
    fn partition(&mut self, a: &[&str], b: &[&str]) {
        switchy::network::partition(a, b);
    }

    /// Block messages from the nodes of `from` to those of `to` only (see
    /// [`switchy::network::partition_one_way`]).
    fn partition_one_way(&mut self, from: &[&str], to: &[&str]) {
        switchy::network::partition_one_way(from, to);
    }

    /// Cut `node` off from every other node (see [`switchy::network::isolate`]).
    fn isolate(&mut self, node: &str) {
        switchy::network::isolate(node);
    }

    /// Heal every partition and isolation (see [`switchy::network::heal`]).
    fn heal(&mut self) {
        switchy::network::heal();
    }

    /// Set the policy of the link from `from` to `to` (see
    /// [`switchy::network::set_link_policy`]).
    fn set_link_policy(&mut self, from: &str, to: &str, policy: switchy::network::LinkPolicy) {
        switchy::network::set_link_policy(from, to, policy);
    }

    /// Set the policy of links without one of their own (see
    /// [`switchy::network::set_default_policy`]).
    fn set_default_link_policy(&mut self, policy: switchy::network::LinkPolicy) {
        switchy::network::set_default_policy(policy);
    }

    /// Spawns a host actor with the given name and action.
    ///
    /// The action is a factory function that returns a future representing
    /// the host's behavior.
    fn host<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
        &mut self,
        name: impl Into<String>,
        action: F,
    );

    /// Spawns a client actor with the given name and action.
    ///
    /// The action is a future representing the client's behavior.
    fn client(
        &mut self,
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + 'static,
    );
}

struct ManagedSim {
    config: SimConfig,
    hosts: Vec<Host>,
    clients: Vec<Client>,
    start: Option<SystemTime>,
    /// Hosts to restart, and when, in the order their bounces were applied.
    restarts: Vec<(SystemTime, String)>,
}

impl ManagedSim {
    const fn new(config: SimConfig) -> Self {
        Self {
            config,
            hosts: vec![],
            clients: vec![],
            start: None,
            restarts: vec![],
        }
    }

    /// Apply the queued host lifecycle changes, then the restarts that are due.
    fn apply_faults(&mut self) {
        for command in fault::take_commands() {
            match command {
                fault::Command::Crash(host) => self.crash_host(&host),
                fault::Command::Restart(host) => self.restart_host(&host),
                fault::Command::Bounce(host, delay) => {
                    self.crash_host(&host);
                    if delay.is_zero() {
                        self.restart_host(&host);
                    } else {
                        let at = switchy::time::now() + delay;
                        self.restarts.push((at, host));
                    }
                }
            }
        }
        let now = switchy::time::now();
        let (due, later) = std::mem::take(&mut self.restarts)
            .into_iter()
            .partition::<Vec<_>, _>(|(at, _)| *at <= now);
        self.restarts = later;
        for (_, host) in due {
            self.restart_host(&host);
        }
    }

    fn crash_host(&mut self, name: &str) {
        let Some(host) = self.hosts.iter_mut().find(|x| x.name == name) else {
            log::warn!("cannot crash host={name}: no such host");
            return;
        };
        if !host.is_running() {
            log::debug!("not crashing host={name}: it is not running");
            return;
        }
        log::debug!("crashing host={name}");
        // Its network node goes down first, so nothing it owned sends or receives again.
        switchy::network::node_down(name);
        host.crash();
        fault::set_status(name, fault::HostStatus::Down);
        fault::run_crash_hooks(name);
    }

    fn restart_host(&mut self, name: &str) {
        let Some(host) = self.hosts.iter_mut().find(|x| x.name == name) else {
            log::warn!("cannot restart host={name}: no such host");
            return;
        };
        if !host.is_down() {
            log::debug!("not restarting host={name}: it is not down");
            return;
        }
        log::debug!("restarting host={name}");
        switchy::network::node_up(name);
        host.start();
        fault::set_status(name, fault::HostStatus::Running);
    }

    pub fn elapsed(&self) -> Duration {
        let Some(start) = self.start else {
            return Duration::ZERO;
        };
        switchy::time::now().duration_since(start).unwrap()
    }

    pub fn start(&mut self) {
        self.start = Some(switchy::time::now());

        for host in self.hosts.iter_mut().filter(|x| x.is_down()) {
            host.start();
            fault::set_status(&host.name, fault::HostStatus::Running);
        }
        for client in self.clients.iter_mut().filter(|x| !x.has_started()) {
            client.start();
        }
    }

    pub fn step(&mut self) -> Result<bool, Error> {
        log::trace!("step {}", current_step());

        self.apply_faults();

        let mut actors = self
            .hosts
            .iter()
            .map(|x| Box::new(x) as Box<dyn Actor>)
            .chain(self.clients.iter().map(|x| Box::new(x) as Box<dyn Actor>))
            .collect::<Vec<_>>();

        if self.config.enable_random_order {
            actors.shuffle(&mut rng());
        }

        for actor in actors {
            actor.tick();
        }

        // Simulated threads (`spawn_blocking` work among them) run too, until none can make
        // progress at this step's time.
        #[cfg(feature = "thread")]
        if let Some(scheduler) = switchy::thread::simulator::try_scheduler()
            && scheduler.others_alive()
        {
            scheduler.run_until_idle();
        }

        let mut remaining_hosts = vec![];

        for mut host in self.hosts.drain(..) {
            // A crashed host stays, to be restarted.
            if host.is_running() || host.is_down() {
                remaining_hosts.push(host);
                continue;
            }
            fault::set_status(&host.name, fault::HostStatus::Finished);
            host.join()?.transpose().map_err(Error::Step)?;
        }

        self.hosts = remaining_hosts;

        let mut remaining_clients = vec![];

        for mut client in self.clients.drain(..) {
            if client.is_running() {
                remaining_clients.push(client);
                continue;
            }
            if let Some(handle) = client.handle {
                client
                    .runtime
                    .block_on(handle)?
                    .transpose()
                    .map_err(Error::Step)?;
            }
        }

        self.clients = remaining_clients;

        if is_simulator_cancelled() {
            log::debug!("cancelled!");
            let client_count = self.clients.len();
            for (i, client) in self.clients.drain(..).enumerate() {
                log::debug!("cancelling client {}/{client_count}!", i + 1);
                if let Some(handle) = client.handle {
                    client
                        .runtime
                        .block_on(handle)?
                        .transpose()
                        .map_err(Error::Step)?;
                }
            }

            let host_count = self.hosts.len();
            for (i, mut host) in self.hosts.drain(..).enumerate() {
                log::debug!("cancelling host {}/{host_count}!", i + 1);
                fault::set_status(&host.name, fault::HostStatus::Finished);
                host.join()?.transpose().map_err(Error::Step)?;
            }
            self.restarts.clear();
        }

        if current_step().is_multiple_of(1000) || END_SIM.load(Ordering::SeqCst) {
            log::debug!("hosts={} clients={}", self.hosts.len(), self.clients.len());
        }

        // Crashed hosts keep the simulation going only while a restart is coming.
        let hosts_done = self.hosts.iter().all(|host| {
            host.is_down() && !self.restarts.iter().any(|(_, name)| *name == host.name)
        });
        Ok(hosts_done && self.clients.is_empty() && !fault::has_commands())
    }

    #[allow(clippy::unused_self)]
    fn shutdown(self) {
        cancel_simulation();
    }
}

impl Sim for ManagedSim {
    fn bounce(&mut self, host: impl Into<String>) {
        let host = host.into();
        log::debug!("bouncing host={host}");
        fault::bounce(host);
    }

    fn host<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
        &mut self,
        name: impl Into<String>,
        action: F,
    ) {
        let name = name.into();
        log::debug!("starting host with name={name}");
        let mut host = Host::new(name, action);
        // Hosts added once the simulation runs start at once.
        if self.start.is_some() {
            host.start();
            fault::set_status(&host.name, fault::HostStatus::Running);
        }
        self.hosts.push(host);
    }

    fn client(
        &mut self,
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + 'static,
    ) {
        let name = name.into();
        log::debug!("starting client with name={name}");
        let mut client = Client::new(name, action);
        // Clients added once the simulation runs start at once.
        if self.start.is_some() {
            client.start();
        }
        self.clients.push(client);
    }
}

pub(crate) trait Actor {
    fn tick(&self);
}

#[cfg(test)]
mod tests;
