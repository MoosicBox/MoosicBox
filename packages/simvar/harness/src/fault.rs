//! Fault injection: crash, restart and bounce hosts, and break the simulated network.
//!
//! These functions act on the current simulation run and can be called from anywhere in it:
//! from [`SimBootstrap::on_step`](crate::SimBootstrap::on_step), and from inside clients and
//! hosts — a *nemesis* client is the usual way to inject faults on a schedule:
//!
//! ```rust,no_run
//! use std::time::Duration;
//!
//! use simvar_harness::{Sim, SimBootstrap, fault, switchy};
//!
//! struct Bootstrap;
//!
//! impl SimBootstrap for Bootstrap {
//!     fn on_start(&self, sim: &mut impl Sim) {
//!         for name in ["n1", "n2", "n3"] {
//!             sim.host(name, || async { Ok(()) });
//!         }
//!         sim.client("nemesis", async {
//!             switchy::unsync::time::sleep(Duration::from_secs(1)).await;
//!             fault::network::partition(["n1"], ["n2", "n3"]);
//!             switchy::unsync::time::sleep(Duration::from_secs(1)).await;
//!             fault::network::heal();
//!             fault::bounce_after("n2", Duration::from_millis(500));
//!             Ok(())
//!         });
//!     }
//! }
//! ```
//!
//! Host lifecycle changes ([`crash`], [`restart`], [`bounce`], [`bounce_after`]) are queued and
//! applied, in order, at the start of the next simulation step, between actor ticks. Network
//! faults ([`network`]) apply at once.
//!
//! A crash is abrupt, and happens in this order:
//!
//! 1. The host's simulated network node goes down, dropping its messages in flight and
//!    resetting its TCP connections.
//! 2. Its action and every task it spawned (`spawn`, `spawn_local`, `JoinSet`) are dropped, in
//!    the order they were spawned, without being polled again. Their destructors run as the
//!    host (in its TCP host and network node scope).
//! 3. With the `thread` feature, the simulated threads its tasks started (`spawn_blocking`, or
//!    `switchy_thread` spawns from a task) are killed in the order they were spawned: each is
//!    stopped at its current switch point and unwound, so the values on its stack are dropped
//!    (releasing its locks) and none of its own code runs again. A blocking closure that blocks
//!    again in a destructor while it unwinds is parked for good, keeping what it still holds.
//! 4. Its crash hooks run ([`on_crash`] hooks first, then those the crashed incarnation
//!    registered with [`host::on_crash`]). Hooks are
//! where a simulation crashes whatever else a host owns, such as its simulated filesystem.
//! A restart runs the host's factory again on a fresh runtime.
//!
//! [`host::on_crash`]: crate::host::on_crash

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Mutex, PoisonError},
    time::Duration,
};

/// The simulated network of the current run; see [`switchy::network`].
pub use switchy::network;

/// A crash hook: called with the name of the crashed host.
type Hook = Box<dyn FnMut(&str) + Send>;

/// Where a host is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostStatus {
    /// Its action is running.
    Running,
    /// Crashed, and not restarted yet.
    Down,
    /// Its action returned (or the simulation was cancelled).
    Finished,
}

/// A queued host lifecycle change.
pub(crate) enum Command {
    Crash(String),
    Restart(String),
    Bounce(String, Duration),
}

#[derive(Default)]
struct Hooks {
    /// Run on every crash of the host.
    every: Vec<Hook>,
    /// Run on the crash of the host's current incarnation only.
    incarnation: Vec<Hook>,
}

#[derive(Default)]
struct State {
    commands: VecDeque<Command>,
    hooks: BTreeMap<String, Hooks>,
    status: BTreeMap<String, HostStatus>,
}

/// The current run's fault state.
#[derive(Default)]
struct Faults(Mutex<State>);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let faults = switchy::simulation::state::<Faults>();
    let mut state = faults.0.lock().unwrap_or_else(PoisonError::into_inner);
    f(&mut state)
}

fn queue(command: Command) {
    with_state(|state| state.commands.push_back(command));
}

/// Crash `host` and leave it down until [restarted](restart). Does nothing if it is not
/// running.
pub fn crash(host: impl Into<String>) {
    queue(Command::Crash(host.into()));
}

/// Restart `host` from its factory, on a fresh runtime. Does nothing if it is running.
pub fn restart(host: impl Into<String>) {
    queue(Command::Restart(host.into()));
}

/// Crash `host` and restart it at once.
pub fn bounce(host: impl Into<String>) {
    bounce_after(host, Duration::ZERO);
}

/// Crash `host` and restart it after `delay` of simulated time. The restart happens at the
/// first step at or after then.
pub fn bounce_after(host: impl Into<String>, delay: Duration) {
    queue(Command::Bounce(host.into(), delay));
}

/// Register `hook` to run, with the host's name, every time `host` crashes in this run.
pub fn on_crash(host: impl Into<String>, hook: impl FnMut(&str) + Send + 'static) {
    with_state(|state| {
        state
            .hooks
            .entry(host.into())
            .or_default()
            .every
            .push(Box::new(hook));
    });
}

pub(crate) fn on_incarnation_crash(host: String, hook: impl FnMut(&str) + Send + 'static) {
    with_state(|state| {
        state
            .hooks
            .entry(host)
            .or_default()
            .incarnation
            .push(Box::new(hook));
    });
}

/// Where `host` is in its life, if the run has such a host.
#[must_use]
pub fn host_status(host: &str) -> Option<HostStatus> {
    with_state(|state| state.status.get(host).copied())
}

/// Every host of the run and where it is in its life, by name.
#[must_use]
pub fn hosts() -> Vec<(String, HostStatus)> {
    with_state(|state| {
        state
            .status
            .iter()
            .map(|(name, status)| (name.clone(), *status))
            .collect()
    })
}

pub(crate) fn set_status(host: &str, status: HostStatus) {
    with_state(|state| {
        state.status.insert(host.to_owned(), status);
    });
}

/// Take the queued lifecycle changes.
pub(crate) fn take_commands() -> Vec<Command> {
    with_state(|state| state.commands.drain(..).collect())
}

/// Whether lifecycle changes are queued.
pub(crate) fn has_commands() -> bool {
    with_state(|state| !state.commands.is_empty())
}

/// Run the crash hooks of `host`, and forget those of its crashed incarnation. The hooks run
/// without the fault state locked, so they may inject faults themselves.
pub(crate) fn run_crash_hooks(host: &str) {
    let (mut every, incarnation) = with_state(|state| {
        let hooks = state.hooks.entry(host.to_owned()).or_default();
        (
            std::mem::take(&mut hooks.every),
            std::mem::take(&mut hooks.incarnation),
        )
    });
    for hook in &mut every {
        hook(host);
    }
    for mut hook in incarnation {
        hook(host);
    }
    with_state(|state| {
        let hooks = state.hooks.entry(host.to_owned()).or_default();
        // Hooks registered while these ran come after them.
        every.append(&mut hooks.every);
        hooks.every = every;
    });
}
