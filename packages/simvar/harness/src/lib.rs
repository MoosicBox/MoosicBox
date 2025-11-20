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
//! * `NO_TUI` - Disable terminal UI when set

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

#[cfg(feature = "tui")]
mod tui;

const USE_TUI: bool = cfg!(feature = "tui") && std::option_env!("NO_TUI").is_none();

thread_local! {
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

static RUNS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(1, |x| x.parse::<u64>().unwrap())
});

static END_SIM: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[cfg(feature = "tui")]
static DISPLAY_STATE: LazyLock<tui::DisplayState> = LazyLock::new(tui::DisplayState::new);

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
    if USE_TUI {
        DISPLAY_STATE.exit();
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
/// * If system time went backwards
///
/// # Errors
///
/// * The contents of this function are wrapped in a `catch_unwind` call, so if
///   any panic happens, it will be wrapped into an error on the outer `Result`
/// * If the `Sim` `step` returns an error, we return that in an Ok(Err(e))
#[allow(clippy::let_and_return)]
pub fn run_simulation<B: SimBootstrap>(
    bootstrap: B,
) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
    static MAX_PARALLEL: LazyLock<u64> = LazyLock::new(|| {
        std::env::var("SIMULATOR_MAX_PARALLEL").ok().map_or_else(
            || {
                u64::try_from(
                    std::thread::available_parallelism()
                        .map(Into::into)
                        .unwrap_or(1usize),
                )
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

    #[cfg(feature = "tui")]
    let tui_handle = if USE_TUI {
        Some(tui::spawn(DISPLAY_STATE.clone()))
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

    let sim_orchestrator = SimOrchestrator::new(
        bootstrap,
        runs,
        max_parallel,
        #[cfg(feature = "tui")]
        DISPLAY_STATE.clone(),
    );

    let resp = sim_orchestrator.start();

    #[cfg(feature = "tui")]
    if let Some(tui_handle) = tui_handle {
        tui_handle.join().unwrap()?;
    }

    #[cfg(feature = "tui")]
    if USE_TUI && let Ok(results) = &resp {
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
    #[cfg(feature = "tui")]
    display_state: tui::DisplayState,
}

impl<B: SimBootstrap> SimOrchestrator<B> {
    const fn new(
        bootstrap: B,
        runs: u64,
        max_parallel: u64,
        #[cfg(feature = "tui")] display_state: tui::DisplayState,
    ) -> Self {
        Self {
            bootstrap,
            runs,
            max_parallel,
            #[cfg(feature = "tui")]
            display_state,
        }
    }

    fn start(self) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
        let parallel = std::cmp::min(self.runs, self.max_parallel);
        let run_index = Arc::new(AtomicU64::new(0));

        let bootstrap = Arc::new(self.bootstrap);
        let results = Arc::new(Mutex::new(BTreeMap::new()));

        if self.max_parallel == 0 {
            for run_number in 1..=self.runs {
                let simulation = Simulation::new(
                    &*bootstrap,
                    #[cfg(feature = "tui")]
                    self.display_state.clone(),
                );

                let result = simulation.run(run_number, None);

                results.lock().unwrap().insert(0, result);

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
                #[cfg(feature = "tui")]
                let display_state = self.display_state.clone();

                let handle = std::thread::spawn(move || {
                    let _ = thread_id();
                    let thread_id = worker_thread_id();
                    let simulation = Simulation::new(
                        &*bootstrap,
                        #[cfg(feature = "tui")]
                        display_state.clone(),
                    );

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

                        results.lock().unwrap().insert(thread_id, result);

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
                        log::error!("failed to join thread {i}: {e:?}");
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

struct Simulation<'a, B: SimBootstrap> {
    #[cfg(feature = "tui")]
    display_state: tui::DisplayState,
    bootstrap: &'a B,
}

impl<'a, B: SimBootstrap> Simulation<'a, B> {
    const fn new(
        bootstrap: &'a B,
        #[cfg(feature = "tui")] display_state: tui::DisplayState,
    ) -> Self {
        Self {
            #[cfg(feature = "tui")]
            display_state,
            bootstrap,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run(&self, run_number: u64, thread_id: Option<u64>) -> SimResult {
        if run_number > 1 {
            switchy::random::simulator::reset_seed();
        }

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

        logging::log_message(format!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(&props)
        ));

        let start = switchy::time::now();

        #[cfg(feature = "tui")]
        self.display_state
            .update_sim_state(thread_id.unwrap_or(1), run_number, config, 0.0, false);

        self.bootstrap.on_start(&mut managed_sim);

        let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let print_step = |sim: &ManagedSim, step| {
                if duration < Duration::MAX {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = (step as f64 / duration_steps as f64).clamp(0.0, 1.0);

                    #[cfg(feature = "tui")]
                    self.display_state.update_sim_state(
                        thread_id.unwrap_or(1),
                        run_number,
                        config,
                        progress,
                        false,
                    );

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

                    self.bootstrap.on_step(&mut managed_sim);

                    #[cfg(feature = "tui")]
                    self.display_state
                        .update_sim_step(thread_id.unwrap_or(1), step);
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

        #[cfg(feature = "tui")]
        self.display_state.run_completed();

        log::debug!("after simulation run");

        let run = SimRunProperties {
            steps,
            real_time_millis,
            sim_time_millis,
        };

        managed_sim.shutdown();

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

        if !result.is_success() {
            end_sim();
        }

        #[cfg(feature = "tui")]
        self.display_state
            .update_sim_step(thread_id.unwrap_or(1), steps);
        #[cfg(feature = "tui")]
        self.display_state.update_sim_state(
            thread_id.unwrap_or(1),
            run_number,
            config,
            #[allow(clippy::cast_precision_loss)]
            if duration < Duration::MAX {
                (current_step() as f64 / duration_steps as f64).clamp(0.0, 1.0)
            } else {
                0.0
            },
            !result.is_success(),
        );

        logging::log_message(result.to_string());

        result
    }
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
