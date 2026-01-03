//! Host actor types and utilities.
//!
//! This module provides the [`Host`] type for modeling persistent actors that can be
//! restarted during a simulation. Hosts are ideal for representing long-running services
//! like servers, databases, or any component that should be able to recover from failures.
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

use std::pin::Pin;

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
#[allow(unused)]
#[must_use]
pub fn current_host() -> Option<String> {
    if HANDLE.is_set() {
        Some(HANDLE.with(|x| x.name.clone()))
    } else {
        None
    }
}

fn with_host<T>(name: String, f: impl FnOnce(&str) -> T) -> T {
    let host = Handle { name };
    HANDLE.set(&host, || f(&host.name))
}

/// Result type for host actions.
///
/// Hosts return `Ok(())` on success or an error on failure.
pub type HostResult = Result<(), Box<dyn std::error::Error + Send + 'static>>;

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
    pub(crate) action: Box<dyn Fn() -> Pin<Box<dyn Future<Output = HostResult> + 'static>>>,
    pub(crate) handle: Option<JoinHandle<Option<HostResult>>>,
    pub(crate) runtime: runtime::Runtime,
}

impl Host {
    pub(crate) fn new<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
        name: impl Into<String>,
        action: F,
    ) -> Self {
        let runtime = runtime::Runtime::new();
        let action = std::rc::Rc::new(action);
        let name = name.into();
        Self {
            name: name.clone(),
            action: Box::new(move || {
                let action = action.clone();
                let name = name.clone();
                Box::pin(async move {
                    with_tcp_host(name.clone(), |name| {
                        log::debug!("starting tcp host on name={name}");
                        with_host(name.to_string(), |name| {
                            log::debug!("starting host on name={name}");
                            action()
                        })
                    })
                    .await
                })
            }),
            handle: None,
            runtime,
        }
    }

    pub(crate) fn start(&mut self) {
        assert!(!self.has_started(), "Host {} already started", self.name);

        self.handle = Some(
            self.runtime
                .spawn_local(run_until_simulation_cancelled((self.action)())),
        );
    }

    pub(crate) const fn has_started(&self) -> bool {
        self.handle.is_some()
    }

    pub(crate) fn is_running(&mut self) -> bool {
        self.handle.as_mut().is_some_and(|x| !x.is_finished())
    }
}

impl Actor for Host {
    fn tick(&self) {
        with_tcp_host(self.name.clone(), |_| {
            with_host(self.name.clone(), |_| self.runtime.tick());
        });
    }
}

impl Actor for &Host {
    fn tick(&self) {
        (*self).tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_current_host_returns_none_outside_context() {
        // When not in a host context, current_host should return None
        assert_eq!(current_host(), None);
    }

    #[test_log::test]
    fn test_current_host_returns_name_inside_context() {
        // When inside a host context via with_host, current_host should return the host name
        let result = with_host("test-server".to_string(), |_name| current_host());

        assert_eq!(result, Some("test-server".to_string()));
    }

    #[test_log::test]
    fn test_current_host_returns_correct_name_for_different_hosts() {
        // Verify different host names are correctly returned
        let result1 = with_host("server-1".to_string(), |_| current_host());
        let result2 = with_host("server-2".to_string(), |_| current_host());

        assert_eq!(result1, Some("server-1".to_string()));
        assert_eq!(result2, Some("server-2".to_string()));
    }

    #[test_log::test]
    fn test_current_host_nested_contexts() {
        // When nested, the innermost context should be visible
        let outer_result = with_host("outer-host".to_string(), |_| {
            // First, verify outer context is visible
            let outer_visible = current_host();

            // Now nest another context
            let inner_result = with_host("inner-host".to_string(), |_| current_host());

            // After inner context exits, outer should be visible again
            let after_inner = current_host();

            (outer_visible, inner_result, after_inner)
        });

        assert_eq!(outer_result.0, Some("outer-host".to_string()));
        assert_eq!(outer_result.1, Some("inner-host".to_string()));
        assert_eq!(outer_result.2, Some("outer-host".to_string()));
    }

    #[test_log::test]
    fn test_with_host_closure_receives_name() {
        // Verify that the closure passed to with_host receives the correct name
        let received_name = with_host("my-server".to_string(), str::to_owned);

        assert_eq!(received_name, "my-server");
    }

    #[test_log::test]
    fn test_current_host_empty_name() {
        // Verify empty string is handled correctly
        let result = with_host(String::new(), |_| current_host());

        assert_eq!(result, Some(String::new()));
    }

    #[test_log::test]
    fn test_current_host_special_characters_in_name() {
        // Verify host names with special characters work correctly
        let special_name = "host:8080/path?query=1&other=2".to_string();
        let result = with_host(special_name.clone(), |_| current_host());

        assert_eq!(result, Some(special_name));
    }
}
