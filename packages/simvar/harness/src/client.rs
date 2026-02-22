//! Client actor types and utilities.
//!
//! This module provides the [`Client`] type for modeling ephemeral actors in simulations.
//! Clients run once and cannot be restarted, making them ideal for representing temporary
//! tasks or one-time operations in a distributed system simulation.
//!
//! # Example
//!
//! ```rust,no_run
//! # use simvar_harness::{SimBootstrap, Sim, SimConfig};
//! # struct MyBootstrap;
//! # impl SimBootstrap for MyBootstrap {
//! #     fn build_sim(&self, config: SimConfig) -> SimConfig { config }
//! #     fn on_start(&self, sim: &mut impl Sim) {
//! sim.client("test-client", async {
//!     // Client logic here
//!     Ok(())
//! });
//! #     }
//! # }
//! ```

use std::pin::Pin;

use scoped_tls::scoped_thread_local;
use simvar_utils::run_until_simulation_cancelled;
use switchy::unsync::{futures::FutureExt as _, runtime, task::JoinHandle};

use crate::Actor;

struct Handle {
    name: String,
}

scoped_thread_local! {
    static HANDLE: Handle
}

/// Returns the name of the currently executing client, if any.
///
/// This function is only meaningful when called from within a client's action
/// future. Returns `None` if called from outside a client context.
#[allow(unused)]
#[must_use]
pub fn current_client() -> Option<String> {
    if HANDLE.is_set() {
        Some(HANDLE.with(|x| x.name.clone()))
    } else {
        None
    }
}

fn with_client<T>(name: String, f: impl FnOnce(&str) -> T) -> T {
    let client = Handle { name };
    HANDLE.set(&client, || f(&client.name))
}

/// Result type for client actions.
///
/// Clients return `Ok(())` on success or an error on failure.
pub type ClientResult = Result<(), Box<dyn std::error::Error + Send>>;

/// A client actor in the simulation.
///
/// Clients represent ephemeral actors that perform specific tasks and then exit.
/// Unlike hosts, clients cannot be restarted or "bounced". They are created through
/// the [`Sim::client`] method and run until their action completes or the simulation
/// is cancelled.
///
/// This type is opaque and cannot be constructed directly by users.
///
/// [`Sim::client`]: crate::Sim::client
pub struct Client {
    pub(crate) name: String,
    #[allow(clippy::type_complexity)]
    pub(crate) action: Option<Pin<Box<dyn Future<Output = ClientResult>>>>,
    pub(crate) handle: Option<JoinHandle<Option<ClientResult>>>,
    pub(crate) runtime: runtime::Runtime,
}

impl Client {
    pub(crate) fn new(
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + 'static,
    ) -> Self {
        let runtime = runtime::Runtime::new();
        let name = name.into();
        Self {
            name,
            action: Some(Box::pin(
                run_until_simulation_cancelled(action).map(|x| x.unwrap_or(Ok(()))),
            )),
            handle: None,
            runtime,
        }
    }

    pub(crate) fn start(&mut self) {
        assert!(!self.has_started(), "Client {} already started", self.name);

        let Some(action) = self.action.take() else {
            panic!("Client already started");
        };

        self.handle = Some(
            self.runtime
                .spawn_local(run_until_simulation_cancelled(action)),
        );
    }

    const fn has_started(&self) -> bool {
        self.handle.is_some()
    }

    pub(crate) fn is_running(&mut self) -> bool {
        self.handle.as_mut().is_some_and(|x| !x.is_finished())
    }
}

impl Actor for Client {
    fn tick(&self) {
        with_client(self.name.clone(), |_| self.runtime.tick());
    }
}

impl Actor for &Client {
    fn tick(&self) {
        (*self).tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_current_client_returns_none_outside_client_context() {
        // When called outside of any client context (no client action is running),
        // current_client() should return None
        let result = current_client();
        assert!(
            result.is_none(),
            "current_client() should return None when not inside a client action"
        );
    }

    #[test_log::test]
    fn test_with_client_sets_current_client_context() {
        // Verify that with_client correctly sets the client context for the duration
        // of the closure, making current_client() return the client name
        let captured_name = with_client("test-client".to_string(), |name| {
            assert_eq!(name, "test-client");
            current_client()
        });

        assert_eq!(captured_name, Some("test-client".to_string()));
    }

    #[test_log::test]
    fn test_current_client_returns_none_after_with_client_exits() {
        // Verify that after with_client exits, current_client() returns None again
        with_client("temporary-client".to_string(), |_name| ());

        let result = current_client();
        assert!(
            result.is_none(),
            "current_client() should return None after with_client exits"
        );
    }

    #[test_log::test]
    fn test_with_client_nested_contexts() {
        // Verify that nested with_client calls correctly shadow the outer context
        with_client("outer-client".to_string(), |outer_name| {
            assert_eq!(outer_name, "outer-client");
            assert_eq!(current_client(), Some("outer-client".to_string()));

            with_client("inner-client".to_string(), |inner_name| {
                assert_eq!(inner_name, "inner-client");
                // Inner context should shadow outer context
                assert_eq!(current_client(), Some("inner-client".to_string()));
            });

            // After inner exits, outer context should be restored
            assert_eq!(current_client(), Some("outer-client".to_string()));
        });
    }
}
