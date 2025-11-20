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
