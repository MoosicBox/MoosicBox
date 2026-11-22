//! Host actor types and utilities.
//!
//! This module provides the [`Host`] type for modeling persistent actors that can be
//! restarted during a simulation. Hosts are ideal for representing long-running services
//! like servers, databases, or any component that should be able to recover from failures.
//!
//! A host can be [crashed](crate::fault::crash), [restarted](crate::fault::restart) and
//! [bounced](crate::fault::bounce): a crash drops the host's async runtime with every task on
//! it, without running any of them again, and a restart runs the host's factory again on a
//! fresh runtime. Code running in a host can register a hook for its own crash with
//! [`on_crash`].
//!
//! # Example
//!
//! ```rust,no_run
//! # use simvar_harness::{SimBootstrap, Sim, SimConfig};
//! # struct MyBootstrap;
//! # impl SimBootstrap for MyBootstrap {
//! #     fn build_sim(&self, config: SimConfig) -> SimConfig { config }
//! #     fn on_start(&self, sim: &mut impl Sim) {
//! sim.host("server", || async {
//!     // Host factory - returns fresh instance on each restart
//!     // Server logic here
//!     Ok(())
//! });
//! #     }
//! # }
//! ```

use std::{cell::RefCell, pin::Pin, rc::Rc, task::Poll};

use scoped_tls::scoped_thread_local;
use simvar_utils::run_until_simulation_cancelled;
use switchy::{
    tcp::simulator::with_host as with_tcp_host,
    unsync::{runtime, task::JoinHandle},
};

use crate::Actor;

struct Handle {
    name: String,
}

scoped_thread_local! {
    static HANDLE: Handle
}

/// Returns the name of the currently executing host, if any.
///
/// This function is only meaningful when called from within a host's action
/// future. Returns `None` if called from outside a host context.
#[allow(unused, clippy::must_use_candidate)]
pub fn current_host() -> Option<String> {
    if HANDLE.is_set() {
        Some(HANDLE.with(|x| x.name.clone()))
    } else {
        None
    }
}

/// Register `hook` to run when the current host crashes, in its current incarnation only.
///
/// A restarted host registers its hooks again. Hooks run after the host's tasks are dropped,
/// with the host's name. Hooks registered for every incarnation (with
/// [`crate::fault::on_crash`]) run first.
///
/// # Panics
///
/// * If called outside a host
pub fn on_crash(hook: impl FnMut(&str) + Send + 'static) {
    let host = current_host().expect("host::on_crash called outside a host");
    crate::fault::on_incarnation_crash(host, hook);
}

/// Run `f` as the host `name`: in its simulated TCP host scope, as its host, and as its node
/// of the simulated network.
fn in_host_scope<T>(name: &str, f: impl FnOnce() -> T) -> T {
    with_tcp_host(name.to_owned(), |name| {
        let host = Handle {
            name: name.to_owned(),
        };
        HANDLE.set(&host, || switchy::network::with_node(name, f))
    })
}

/// Result type for host actions.
///
/// Hosts return `Ok(())` on success or an error on failure.
pub type HostResult = Result<(), Box<dyn std::error::Error + Send + 'static>>;

type HostFuture = Pin<Box<dyn Future<Output = Option<HostResult>> + 'static>>;

/// One life of a host, from a start to a crash (or to the end of its action).
struct Incarnation {
    runtime: runtime::Runtime,
    /// The host's action. A crash drops it at once, without polling it again.
    future: Rc<RefCell<Option<HostFuture>>>,
    handle: JoinHandle<Option<HostResult>>,
}

/// A host actor in the simulation.
///
/// Hosts represent persistent actors that can be restarted (bounced) during a simulation.
/// They are created through the [`Sim::host`] method with a factory function that allows
/// them to be restarted with fresh state.
///
/// This type is opaque and cannot be constructed directly by users.
///
/// [`Sim::host`]: crate::Sim::host
pub struct Host {
    pub(crate) name: String,
    #[allow(clippy::type_complexity)]
    action: Box<dyn Fn() -> Pin<Box<dyn Future<Output = HostResult> + 'static>>>,
    incarnation: Option<Incarnation>,
}

impl Host {
    pub(crate) fn new<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
        name: impl Into<String>,
        action: F,
    ) -> Self {
        let action = Rc::new(action);
        let name = name.into();
        Self {
            name: name.clone(),
            action: Box::new(move || {
                let action = action.clone();
                let name = name.clone();
                Box::pin(async move {
                    log::debug!("starting host on name={name}");
                    in_host_scope(&name, || action()).await
                })
            }),
            incarnation: None,
        }
    }

    /// Start a new incarnation on a fresh runtime.
    pub(crate) fn start(&mut self) {
        assert!(!self.is_running(), "Host {} already running", self.name);

        let runtime = runtime::Runtime::new();
        let future: HostFuture = Box::pin(run_until_simulation_cancelled((self.action)()));
        let future = Rc::new(RefCell::new(Some(future)));
        let slot = Rc::clone(&future);
        let handle = runtime.spawn_local(std::future::poll_fn(move |cx| {
            let mut slot = slot.borrow_mut();
            let Some(future) = slot.as_mut() else {
                // Crashed: never resolves, and is never polled again.
                return Poll::Pending;
            };
            let poll = future.as_mut().poll(cx);
            if poll.is_ready() {
                *slot = None;
            }
            poll
        }));
        self.incarnation = Some(Incarnation {
            runtime,
            future,
            handle,
        });
    }

    /// Kill the current incarnation: drop its action now and abandon its runtime, so none of
    /// its tasks runs again.
    pub(crate) fn crash(&mut self) {
        let Some(incarnation) = self.incarnation.take() else {
            return;
        };
        let future = incarnation.future.borrow_mut().take();
        // Drop the action as the host, for anything its destructors do.
        in_host_scope(&self.name, || drop(future));
        drop(incarnation);
    }

    /// Whether an incarnation is running.
    pub(crate) fn is_running(&self) -> bool {
        self.incarnation
            .as_ref()
            .is_some_and(|x| !x.handle.is_finished())
    }

    /// Whether the host is down: crashed, or not started yet.
    pub(crate) const fn is_down(&self) -> bool {
        self.incarnation.is_none()
    }

    /// The result of the current incarnation, waiting for it to finish if need be.
    pub(crate) fn join(&mut self) -> Result<Option<HostResult>, switchy::unsync::task::JoinError> {
        let Some(incarnation) = self.incarnation.take() else {
            return Ok(None);
        };
        incarnation.runtime.block_on(incarnation.handle)
    }
}

impl Actor for Host {
    fn tick(&self) {
        if let Some(incarnation) = &self.incarnation {
            in_host_scope(&self.name, || incarnation.runtime.tick());
        }
    }
}

impl Actor for &Host {
    fn tick(&self) {
        (*self).tick();
    }
}
