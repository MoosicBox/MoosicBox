//! Basic simulation example demonstrating core simvar concepts.
//!
//! This example showcases the fundamentals of the simvar simulation framework:
//!
//! * Creating host and client actors
//! * Using simulation time for deterministic execution
//! * Message passing between actors
//! * Deterministic, reproducible test scenarios
//!
//! # Usage
//!
//! Run this example with:
//!
//! ```bash
//! cargo run --package simvar_basic_simulation_example
//! ```
//!
//! The simulation will run for 5 seconds of simulated time, demonstrating
//! basic actor interaction and time-based execution.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use simvar::{Sim, SimBootstrap, SimConfig, run_simulation};
use switchy_async::tokio;

/// Simple simulation demonstrating host/client actors and deterministic execution.
///
/// This simulation creates:
/// * A persistent "server" host that tracks connection counts
/// * Multiple "client" actors that connect periodically
/// * Deterministic time-based execution
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and run the simulation
    let bootstrap = BasicSimulationBootstrap::new();
    let results = run_simulation(bootstrap)?;

    // Display results
    println!("\n=== SIMULATION RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    Ok(())
}

/// Bootstrap configuration for the basic simulation.
///
/// This struct holds the simulation configuration and implements the `SimBootstrap`
/// trait to set up the simulation environment.
struct BasicSimulationBootstrap {
    /// Number of client actors to create
    client_count: usize,
    /// Shared counter for tracking connections
    connection_counter: Arc<AtomicU32>,
}

impl BasicSimulationBootstrap {
    /// Creates a new bootstrap configuration with default values.
    #[must_use]
    fn new() -> Self {
        Self {
            client_count: 3,
            connection_counter: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl SimBootstrap for BasicSimulationBootstrap {
    /// Configures the simulation parameters.
    ///
    /// Sets the simulation to run for 5 seconds of simulated time.
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(5);
        config
    }

    /// Initializes the simulation by spawning host and client actors.
    ///
    /// This method is called once at the start of the simulation to set up
    /// the initial actors and their behavior.
    fn on_start(&self, sim: &mut impl Sim) {
        // Clone the counter for use in closures
        let counter = Arc::clone(&self.connection_counter);

        // Spawn a persistent host actor
        // Hosts are long-running services that exist for the simulation duration
        sim.host("server", move || {
            // Clone counter for the async block
            let counter = Arc::clone(&counter);

            Box::pin(async move {
                log::info!("Server starting up...");

                // Server loop: run until simulation ends
                loop {
                    // Wait for a short interval
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    // Check connection count periodically
                    let count = counter.load(Ordering::Relaxed);
                    log::debug!("Server: Current connection count = {count}");
                }

                // This code is unreachable in practice, as the simulation duration
                // will terminate the host, but we need to return a Result
                #[allow(unreachable_code)]
                Ok::<(), Box<dyn std::error::Error + Send + 'static>>(())
            })
        });

        // Spawn multiple client actors
        // Clients are ephemeral entities that perform specific tasks
        for i in 0..self.client_count {
            let counter = Arc::clone(&self.connection_counter);

            sim.client(format!("client-{i}"), async move {
                log::info!("Client {i} starting...");

                // Wait a bit before first connection (stagger client starts)
                let initial_delay = Duration::from_millis(100 * u64::try_from(i).unwrap_or(0));
                tokio::time::sleep(initial_delay).await;

                // Simulate periodic "connections" to the server
                for round in 0..3 {
                    // "Connect" to server by incrementing the counter
                    let prev = counter.fetch_add(1, Ordering::Relaxed);
                    log::info!(
                        "Client {i}: Connection #{round} (total connections: {})",
                        prev + 1
                    );

                    // Do some "work" (simulate processing time)
                    tokio::time::sleep(Duration::from_millis(800)).await;

                    // "Disconnect" by decrementing the counter
                    let prev = counter.fetch_sub(1, Ordering::Relaxed);
                    log::debug!(
                        "Client {i}: Disconnected (remaining connections: {})",
                        prev - 1
                    );

                    // Wait before next connection attempt
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }

                log::info!("Client {i} completed all connections");

                Ok::<(), Box<dyn std::error::Error + Send>>(())
            });
        }
    }

    /// Called when the simulation ends.
    ///
    /// Reports final statistics about the simulation run.
    fn on_end(&self, _sim: &mut impl Sim) {
        let final_count = self.connection_counter.load(Ordering::Relaxed);
        println!("\n=== FINAL STATISTICS ===");
        println!("Total clients: {}", self.client_count);
        println!("Active connections at end: {final_count}");
        println!("(All clients should have disconnected)");
    }
}
