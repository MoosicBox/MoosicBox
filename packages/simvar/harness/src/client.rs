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
    fn test_current_client_returns_none_outside_context() {
        // current_client should return None when not inside a with_client scope
        assert!(current_client().is_none());
    }

    #[test_log::test]
    fn test_current_client_returns_name_inside_context() {
        // Test that current_client returns the correct name when inside with_client scope
        let result = with_client("test-client".to_string(), |_| current_client());
        assert_eq!(result, Some("test-client".to_string()));
    }

    #[test_log::test]
    fn test_current_client_nested_contexts() {
        // Test nested with_client calls - inner context should take precedence
        let result = with_client("outer".to_string(), |_| {
            with_client("inner".to_string(), |_| current_client())
        });
        assert_eq!(result, Some("inner".to_string()));
    }

    #[test_log::test]
    fn test_current_client_restores_after_context() {
        // After exiting with_client scope, current_client should return None
        with_client("test".to_string(), |_| {
            assert!(current_client().is_some());
        });
        assert!(current_client().is_none());
    }

    #[test_log::test]
    fn test_with_client_passes_name_to_closure() {
        // Verify the name passed to with_client is available in the closure
        let received_name = with_client("my-client".to_string(), std::string::ToString::to_string);
        assert_eq!(received_name, "my-client");
    }

    #[test_log::test]
    fn test_with_client_returns_closure_result() {
        // Verify with_client returns the result from the closure
        let result = with_client("client".to_string(), |_| 42);
        assert_eq!(result, 42);
    }
}
