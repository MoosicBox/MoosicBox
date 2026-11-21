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
//! * `SIMULATOR_RUNS` - Number of simulation runs to execute (default: 1)
//! * `SIMULATOR_MAX_PARALLEL` - Maximum parallel runs (default: number of CPUs)
//! * `SIMULATOR_SEED` - Fixed random seed for deterministic runs
//! * `SIMULATOR_EPOCH_OFFSET` - Fixed epoch offset override (when `time` feature is enabled)
//! * `SIMULATOR_EPOCH_MIN` / `SIMULATOR_EPOCH_MAX` - Bounded random epoch offset (inclusive)
//! * `SIMULATOR_EPOCH_RANGE_PROFILE` - Epoch profile (`low`, `wide`, `full`)
//! * `SIMULATOR_VERIFY_DETERMINISM` - Replay every run and fail if its trace differs
//! * `SIMULATOR_SHRINK` - With the `thread` feature: shrink a failing run's thread schedule
//!   (`1`/`true` for up to 1000 replays, or a number of replays) and print how to replay it
//! * `NO_TUI` - Disable terminal UI when set (at build or run time); it is also off when stdout is not a terminal

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    panic::AssertUnwindSafe,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, SystemTime},
};

use client::{Client, ClientResult};
use color_backtrace::{BacktracePrinter, termcolor::Buffer};
use config::run_info;
use formatting::TimeFormat as _;
use host::{Host, HostResult};
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

/// Time formatting utilities.
///
/// Provides the [`TimeFormat`] trait for converting time durations
/// in milliseconds into human-readable formatted strings.
///
/// [`TimeFormat`]: formatting::TimeFormat
pub mod formatting;

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

static RUNS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(1, |x| x.parse::<u64>().unwrap())
});

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
/// simulation runs to terminate gracefully.
pub fn end_sim() {
    END_SIM.store(true, std::sync::atomic::Ordering::SeqCst);

    if !is_global_simulator_cancelled() {
        cancel_global_simulation();
    }
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

    std::panic::set_hook(Box::new({
        move |x| {
            let thread_id = thread_id();
            let mut panic_str = x.to_string();
            if let Some(bt) = try_get_backtrace() {
                panic_str = format!("{panic_str}\n{bt}");
            }
            log::debug!("caught panic on thread_id={thread_id}: {panic_str}");
            PANIC.with_borrow_mut(|x| *x = Some(panic_str));
            end_sim();
        }
    }));

    let runs = *RUNS;
    let max_parallel = *MAX_PARALLEL;

    log::debug!("Running simulation with max_parallel={max_parallel}");

    notify(&simvar_observer::SimEvent::Started { runs, max_parallel });
    let sim_orchestrator = SimOrchestrator::new(bootstrap, runs, max_parallel);

    let resp = sim_orchestrator.start();
    notify(&simvar_observer::SimEvent::Finished);

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
    runs: u64,
    max_parallel: u64,
}

impl<B: SimBootstrap> SimOrchestrator<B> {
    const fn new(bootstrap: B, runs: u64, max_parallel: u64) -> Self {
        Self {
            bootstrap,
            runs,
            max_parallel,
        }
    }

    fn start(self) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
        let parallel = std::cmp::min(self.runs, self.max_parallel);
        let run_index = Arc::new(AtomicU64::new(0));

        let bootstrap = Arc::new(self.bootstrap);
        // Every run's result, in run order.
        let results = Arc::new(Mutex::new(BTreeMap::new()));

        if self.max_parallel == 0 {
            for run_number in 1..=self.runs {
                let simulation = Simulation::new(&*bootstrap);

                let result = simulation.run(run_number, None);

                results.lock().unwrap().insert(run_number, result);

                if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
        } else {
            let mut threads = vec![];

            for i in 0..parallel {
                log::debug!("starting thread {i}");

                let run_index = run_index.clone();
                let bootstrap = bootstrap.clone();
                let runs = self.runs;
                let results = results.clone();

                let handle = std::thread::spawn(move || {
                    let _ = thread_id();
                    let thread_id = worker_thread_id();
                    let simulation = Simulation::new(&*bootstrap);

                    loop {
                        if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
                            log::debug!("simulation has ended. thread {i} ({thread_id}) finished");
                            break;
                        }

                        let run_index = run_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if run_index >= runs {
                            log::debug!(
                                "finished all runs ({runs}). thread {i} ({thread_id}) finished"
                            );
                            break;
                        }

                        log::debug!(
                            "starting simulation run_index={run_index} on thread {i} ({thread_id})"
                        );

                        let result = simulation.run(run_index + 1, Some(thread_id));

                        results.lock().unwrap().insert(run_index + 1, result);

                        log::debug!(
                            "simulation finished run_index={run_index} on thread {i} ({thread_id})"
                        );
                    }

                    Ok::<_, String>(())
                });

                threads.push(handle);
            }

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
        }

        Ok(Arc::try_unwrap(results)
            .unwrap()
            .into_inner()
            .unwrap()
            .into_values()
            .collect())
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

    /// Run once (shrinking a failure's schedule if asked to), and with determinism
    /// verification on, run the same seed again and fail if the two runs' traces differ. A
    /// failure ends the simulation.
    fn run(&self, run_number: u64, thread_id: Option<u64>) -> SimResult {
        let result = self.run_checked(run_number, thread_id);
        if !result.is_success() {
            end_sim();
        }
        result
    }

    fn run_checked(&self, run_number: u64, thread_id: Option<u64>) -> SimResult {
        let first = self.run_once(run_number, thread_id, Replay::No);
        if !first.result.is_success() {
            return self.shrunk(run_number, thread_id, first);
        }
        if !verify_determinism() {
            return first.result;
        }
        let again = self.run_once(run_number, thread_id, Replay::Seed(first.seed));
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
    fn shrunk(&self, run_number: u64, thread_id: Option<u64>, failed: RunOutcome) -> SimResult {
        #[cfg(feature = "thread")]
        if let Some(attempts) = shrink_attempts() {
            use switchy::thread::simulator::shrink;
            let seed = failed.seed;
            let original = failed.schedule.choices.len();
            let schedule = shrink(&failed.schedule, attempts, |candidate| {
                let outcome = self.run_once(
                    run_number,
                    thread_id,
                    Replay::Silent(seed, candidate.clone()),
                );
                (!outcome.result.is_success()).then_some(outcome.schedule)
            });
            let last = self.run_once(
                run_number,
                thread_id,
                Replay::Silent(seed, schedule.clone()),
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
                let command = config::get_run_command(
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

    /// One run. `replay` repeats an earlier run's seed instead of advancing the seed sequence,
    /// and can fix its thread schedule (silently: no reports, events or telemetry).
    #[allow(clippy::too_many_lines)]
    fn run_once(&self, run_number: u64, thread_id: Option<u64>, replay: Replay) -> RunOutcome {
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
                // The failure being shrunk cancelled every run; this replay must still run.
                // No new runs start either way (see `END_SIM`).
                simvar_utils::reset_global_simulator_cancellation_token();
                switchy::simulation::set_seed(seed);
                let _ = switchy::simulation::state_with(|| Silent(true));
                let mut config = switchy::thread::simulator::Config::from_env(seed);
                config.strategy = switchy::thread::simulator::Strategy::Replay(Arc::new(schedule));
                switchy::thread::simulator::configure(config);
            }
        }
        let seed = switchy::random::simulator::seed();
        let run_id = simvar_observer::RunId {
            worker: thread_id.unwrap_or(1),
            run: run_number,
        };
        let _ = switchy::simulation::state_with(|| CurrentRun(run_id));

        switchy::random::simulator::reset_rng();
        switchy::tcp::simulator::reset();
        #[cfg(feature = "fs")]
        switchy::fs::simulator::reset_fs();
        #[cfg(feature = "time")]
        switchy::time::simulator::reset_epoch_offset();
        #[cfg(feature = "time")]
        switchy::time::simulator::reset_step_multiplier();
        reset_simulator_cancellation_token();
        reset_step();

        self.bootstrap.init();

        let config = self.bootstrap.build_sim(SimConfig::from_rng());
        let duration = config.duration;
        let duration_steps = duration.as_millis();

        let mut managed_sim = ManagedSim::new(config);

        let props = SimProperties {
            run_number,
            thread_id,
            config,
            extra: self.bootstrap.props(),
        };

        notify(&simvar_observer::SimEvent::RunStarted {
            id: run_id,
            seed,
            props: props.extra.clone(),
            steps: (duration < Duration::MAX)
                .then(|| u64::try_from(duration_steps).unwrap_or(u64::MAX)),
        });

        logging::log_message(format!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(&props)
        ));

        let start = switchy::time::now();

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
                        notify(&simvar_observer::SimEvent::Progress {
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
        let sim_time_millis = managed_sim.elapsed().as_millis();
        let steps = current_step() - 1;

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

        let panic = PANIC.with_borrow(Clone::clone);

        let result = if let Err(e) = resp {
            SimResult::Fail {
                props,
                run,
                error: if panic.is_none() {
                    Some(format!("{e:?}"))
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

        if !result.is_success() {
            // Start no more runs; the run's caller ends the simulation once it is done with
            // this failure (shrinking it, for example).
            END_SIM.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        logging::log_message(result.to_string());

        let trace = mix(trace, u64::from(result_is_success(&result)));
        notify(&simvar_observer::SimEvent::RunFinished {
            id: run_id,
            seed,
            success: result_is_success(&result),
            report: result.to_string(),
            steps,
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

fn notify(event: &simvar_observer::SimEvent) {
    if silent() {
        return;
    }
    let observers = OBSERVERS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for observer in observers.iter() {
        observer.event(event.clone());
    }
}

/// The run the current simulation context belongs to.
struct CurrentRun(simvar_observer::RunId);

/// Send `payload` to every observer as telemetry of the current run. Callable from any thread
/// of the run, including simulated threads. Never changes the run.
pub fn emit(payload: Arc<dyn std::any::Any + Send + Sync>) {
    let id = switchy::simulation::state_with(|| {
        CurrentRun(simvar_observer::RunId { worker: 0, run: 0 })
    })
    .0;
    notify(&simvar_observer::SimEvent::Telemetry { id, payload });
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
    /// Returns custom properties to include in simulation output.
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

/// Interface for managing simulation actors (hosts and clients).
pub trait Sim {
    /// Simulates a host restart by name.
    fn bounce(&mut self, host: impl Into<String>);

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
}

impl ManagedSim {
    const fn new(config: SimConfig) -> Self {
        Self {
            config,
            hosts: vec![],
            clients: vec![],
            start: None,
        }
    }

    pub fn elapsed(&self) -> Duration {
        let Some(start) = self.start else {
            return Duration::ZERO;
        };
        switchy::time::now().duration_since(start).unwrap()
    }

    pub fn start(&mut self) {
        self.start = Some(switchy::time::now());

        for host in self.hosts.iter_mut().filter(|x| !x.has_started()) {
            host.start();
        }
        for client in &mut self.clients {
            client.start();
        }
    }

    pub fn step(&mut self) -> Result<bool, Error> {
        log::trace!("step {}", current_step());
        // if current_step() == 300 {
        //     panic!();
        // }

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

        let mut remaining_hosts = vec![];

        for mut host in self.hosts.drain(..) {
            if host.is_running() {
                remaining_hosts.push(host);
                continue;
            }
            if let Some(handle) = host.handle {
                host.runtime
                    .block_on(handle)?
                    .transpose()
                    .map_err(Error::Step)?;
            }
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
            for (i, host) in self.hosts.drain(..).enumerate() {
                log::debug!("cancelling host {}/{host_count}!", i + 1);
                if let Some(handle) = host.handle {
                    host.runtime
                        .block_on(handle)?
                        .transpose()
                        .map_err(Error::Step)?;
                }
            }
        }

        if current_step().is_multiple_of(1000) || END_SIM.load(std::sync::atomic::Ordering::SeqCst)
        {
            log::debug!("hosts={} clients={}", self.hosts.len(), self.clients.len());
        }

        Ok(self.hosts.is_empty() && self.clients.is_empty())
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
    }

    fn host<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
        &mut self,
        name: impl Into<String>,
        action: F,
    ) {
        let name = name.into();
        log::debug!("starting host with name={name}");
        self.hosts.push(Host::new(name, action));
    }

    fn client(
        &mut self,
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + 'static,
    ) {
        let name = name.into();
        log::debug!("starting client with name={name}");
        self.clients.push(Client::new(name, action));
    }
}

pub(crate) trait Actor {
    fn tick(&self);
}
