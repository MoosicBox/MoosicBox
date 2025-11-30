//! Deterministic simulator for testing the `MoosicBox` server.
//!
//! This crate provides a simulation harness for testing the `MoosicBox` server under various
//! conditions including fault injection and health monitoring. It uses deterministic simulation
//! to enable reproducible testing of distributed system behaviors.
//!
//! # Main Components
//!
//! * [`client`] - Client simulators for fault injection and health checking
//! * [`host`] - Host simulation for running the `MoosicBox` server
//! * [`http`] - HTTP utilities for making requests and parsing responses in simulations
//!
//! # Example
//!
//! ```rust,no_run
//! use moosicbox_server_simulator::{client, handle_actions, host};
//! use simvar::{Sim, SimBootstrap, run_simulation};
//!
//! struct MySimulator;
//!
//! impl SimBootstrap for MySimulator {
//!     fn on_start(&self, sim: &mut impl Sim) {
//!         // Start the MoosicBox server in the simulation
//!         host::moosicbox_server::start(sim, None);
//!
//!         // Start client simulators
//!         client::health_checker::start(sim);
//!         client::fault_injector::start(sim);
//!     }
//!
//!     fn on_step(&self, sim: &mut impl Sim) {
//!         // Handle queued actions (like bounces)
//!         handle_actions(sim);
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Run the simulation
//! let results = run_simulation(MySimulator)?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use simvar::{Sim, switchy::tcp::TcpStream};

pub mod client;
pub mod host;
pub mod http;

static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

enum Action {
    Bounce(String),
}

/// Queues a bounce (restart) action for a host.
///
/// # Panics
///
/// * If the `ACTIONS` `Mutex` fails to lock
pub fn queue_bounce(host: impl Into<String>) {
    ACTIONS
        .lock()
        .unwrap()
        .push_back(Action::Bounce(host.into()));
}

/// Handles all queued actions in the simulation.
///
/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(sim: &mut impl Sim) {
    let actions = ACTIONS.lock().unwrap().drain(..).collect::<Vec<_>>();
    for action in actions {
        match action {
            Action::Bounce(host) => {
                log::debug!("bouncing '{host}'");
                sim.bounce(host);
            }
        }
    }
}

/// Clears all queued actions.
///
/// # Panics
///
/// * If the `ACTIONS` `Mutex` fails to lock
#[cfg(test)]
fn clear_actions() {
    ACTIONS.lock().unwrap().clear();
}

/// Attempts to connect to a TCP stream with retries.
///
/// # Errors
///
/// * If fails to connect to the TCP stream after `max_attempts` tries
pub async fn try_connect(addr: &str, max_attempts: usize) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        tokio::select! {
            resp = TcpStream::connect(addr) => {
                match resp {
                    Ok(x) => break x,
                    Err(e) => {
                        count += 1;

                        log::debug!("failed to bind tcp: {e:?} (attempt {count}/{max_attempts})");

                        if !matches!(e.kind(), std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset)
                            || count >= max_attempts
                        {
                            return Err(e);
                        }

                        tokio::time::sleep(Duration::from_millis(5000)).await;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(5000)) => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timed out after 5000ms"));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use serial_test::serial;
    use simvar::{Sim, client::ClientResult, host::HostResult};

    use super::*;

    /// A mock implementation of the `Sim` trait for testing.
    struct MockSim {
        bounced_hosts: Vec<String>,
    }

    impl MockSim {
        fn new() -> Self {
            Self {
                bounced_hosts: vec![],
            }
        }
    }

    impl Sim for MockSim {
        fn bounce(&mut self, host: impl Into<String>) {
            self.bounced_hosts.push(host.into());
        }

        fn host<F: Fn() -> Fut + 'static, Fut: Future<Output = HostResult> + 'static>(
            &mut self,
            _name: impl Into<String>,
            _action: F,
        ) {
            // Not needed for action handling tests
        }

        fn client(
            &mut self,
            _name: impl Into<String>,
            _action: impl Future<Output = ClientResult> + 'static,
        ) {
            // Not needed for action handling tests
        }
    }

    mod queue_bounce_tests {
        use super::*;

        #[test_log::test]
        #[serial]
        fn queues_single_bounce_action() {
            clear_actions();
            queue_bounce("test_host");

            let actions = ACTIONS.lock().unwrap();
            assert_eq!(actions.len(), 1);
            assert!(matches!(&actions[0], Action::Bounce(h) if h == "test_host"));
            drop(actions);
        }

        #[test_log::test]
        #[serial]
        fn queues_multiple_bounce_actions_in_order() {
            clear_actions();
            queue_bounce("host1");
            queue_bounce("host2");
            queue_bounce("host3");

            let actions = ACTIONS.lock().unwrap();
            assert_eq!(actions.len(), 3);
            assert!(matches!(&actions[0], Action::Bounce(h) if h == "host1"));
            assert!(matches!(&actions[1], Action::Bounce(h) if h == "host2"));
            assert!(matches!(&actions[2], Action::Bounce(h) if h == "host3"));
            drop(actions);
        }

        #[test_log::test]
        #[serial]
        fn accepts_string_and_str_inputs() {
            clear_actions();
            queue_bounce("str_host");
            queue_bounce(String::from("string_host"));

            let actions = ACTIONS.lock().unwrap();
            assert_eq!(actions.len(), 2);
            assert!(matches!(&actions[0], Action::Bounce(h) if h == "str_host"));
            assert!(matches!(&actions[1], Action::Bounce(h) if h == "string_host"));
            drop(actions);
        }
    }

    mod handle_actions_tests {
        use super::*;

        #[test_log::test]
        #[serial]
        fn handles_empty_action_queue() {
            clear_actions();
            let mut sim = MockSim::new();

            handle_actions(&mut sim);

            assert!(sim.bounced_hosts.is_empty());
        }

        #[test_log::test]
        #[serial]
        fn drains_and_processes_all_bounce_actions() {
            clear_actions();
            queue_bounce("host_a");
            queue_bounce("host_b");

            let mut sim = MockSim::new();
            handle_actions(&mut sim);

            // Verify bounces were called on the sim
            assert_eq!(sim.bounced_hosts.len(), 2);
            assert_eq!(sim.bounced_hosts[0], "host_a");
            assert_eq!(sim.bounced_hosts[1], "host_b");

            // Verify queue is now empty
            assert!(ACTIONS.lock().unwrap().is_empty());
        }

        #[test_log::test]
        #[serial]
        fn clears_queue_after_handling() {
            clear_actions();
            queue_bounce("host1");
            queue_bounce("host2");

            let mut sim = MockSim::new();
            handle_actions(&mut sim);

            // Queue should be empty after handling
            assert!(ACTIONS.lock().unwrap().is_empty());

            // Handle again should have no effect
            handle_actions(&mut sim);
            assert_eq!(sim.bounced_hosts.len(), 2); // Still just 2 from before
        }

        #[test_log::test]
        #[serial]
        fn processes_actions_in_fifo_order() {
            clear_actions();
            queue_bounce("first");
            queue_bounce("second");
            queue_bounce("third");

            let mut sim = MockSim::new();
            handle_actions(&mut sim);

            assert_eq!(sim.bounced_hosts, vec!["first", "second", "third"]);
        }
    }
}
