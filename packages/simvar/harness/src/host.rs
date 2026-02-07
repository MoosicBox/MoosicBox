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
    fn test_current_host_returns_none_outside_host_context() {
        // When called outside of a host's action future, current_host should return None
        let result = current_host();
        assert!(
            result.is_none(),
            "current_host should return None when not inside a host context"
        );
    }

    #[test_log::test]
    fn test_current_host_returns_name_within_host_context() {
        // Manually set up the scoped thread-local to simulate being inside a host context
        let expected_name = "test-host";
        let host = Handle {
            name: expected_name.to_string(),
        };

        HANDLE.set(&host, || {
            let result = current_host();
            assert_eq!(
                result,
                Some(expected_name.to_string()),
                "current_host should return the host name when inside a host context"
            );
        });
    }

    #[test_log::test]
    fn test_with_host_provides_name_to_closure() {
        let result = with_host("my-host".to_string(), str::to_uppercase);
        assert_eq!(result, "MY-HOST");
    }

    #[test_log::test]
    fn test_nested_host_contexts() {
        // Test that nested host contexts work correctly (inner overrides outer)
        let outer = Handle {
            name: "outer".to_string(),
        };

        HANDLE.set(&outer, || {
            assert_eq!(current_host(), Some("outer".to_string()));

            let inner = Handle {
                name: "inner".to_string(),
            };

            HANDLE.set(&inner, || {
                assert_eq!(
                    current_host(),
                    Some("inner".to_string()),
                    "Inner context should override outer context"
                );
            });

            // After inner context exits, outer should be visible again
            assert_eq!(
                current_host(),
                Some("outer".to_string()),
                "Outer context should be restored after inner exits"
            );
        });
    }
}
