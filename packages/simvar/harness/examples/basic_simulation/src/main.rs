//! Basic simulation example using `simvar_harness`.
//!
//! This example demonstrates how to use the `simvar_harness` simulation framework to create
//! a simple deterministic simulation with host and client actors. The simulation includes:
//!
//! * A host actor that processes messages and tracks state
//! * Multiple client actors that send messages to the host
//! * Custom simulation configuration and lifecycle hooks
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml
//! ```
//!
//! The simulation runs for 5 seconds with 3 concurrent client actors.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use simvar_harness::{
    Sim, SimBootstrap, SimConfig, client::ClientResult, host::HostResult, run_simulation,
};

/// Example demonstrating basic simulation usage with `simvar_harness`.
///
/// This simulation creates:
/// * A message processor host that tracks processed messages
/// * Multiple client actors that send numbered messages
/// * Lifecycle hooks that log simulation progress
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting basic simulation example...\n");

    let bootstrap = BasicSimulationBootstrap::new();
    let results = run_simulation(bootstrap)?;

    println!("\n=== SIMULATION RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    let success_count = results.iter().filter(|r| r.is_success()).count();
    let total_count = results.len();
    println!("\nSuccess rate: {success_count}/{total_count}");

    if success_count == total_count {
        println!("All simulation runs completed successfully!");
        Ok(())
    } else {
        Err("Some simulation runs failed".into())
    }
}

/// Bootstrap configuration for the basic simulation
struct BasicSimulationBootstrap {
    client_count: usize,
    message_interval: Duration,
}

impl BasicSimulationBootstrap {
    #[must_use]
    const fn new() -> Self {
        Self {
            client_count: 3,
            message_interval: Duration::from_millis(500),
        }
    }
}

impl SimBootstrap for BasicSimulationBootstrap {
    /// Return custom properties to include in simulation output
    fn props(&self) -> Vec<(String, String)> {
        vec![
            ("client_count".to_string(), self.client_count.to_string()),
            (
                "message_interval_ms".to_string(),
                self.message_interval.as_millis().to_string(),
            ),
        ]
    }

    /// Configure simulation parameters
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        // Run simulation for 5 seconds
        config.duration = Duration::from_secs(5);
        // Enable random actor execution order for more realistic testing
        config.enable_random_order = true;
        config
    }

    /// Called once before simulation runs begin
    fn init(&self) {
        log::info!("Initializing basic simulation");
    }

    /// Called when a simulation run starts - spawn actors here
    fn on_start(&self, sim: &mut impl Sim) {
        log::info!("Starting basic simulation run");

        // Start the message processor host
        // Hosts can be restarted during simulation (though not demonstrated here)
        sim.host("message-processor", move || {
            Box::pin(async move { run_message_processor().await })
        });

        // Start multiple client actors that send messages
        for i in 0..self.client_count {
            let client_id = i + 1;
            let message_interval = self.message_interval;

            sim.client(format!("client-{client_id}"), async move {
                run_message_client(client_id, message_interval).await
            });
        }
    }

    /// Called on each simulation step (every millisecond of simulated time)
    fn on_step(&self, _sim: &mut impl Sim) {
        // Optional: Add per-step logic here
        // This is called frequently, so keep it lightweight
    }

    /// Called when a simulation run ends
    fn on_end(&self, _sim: &mut impl Sim) {
        log::info!("Basic simulation run completed");
    }
}

/// Message processor host that receives and processes messages
#[allow(clippy::future_not_send)]
async fn run_message_processor() -> HostResult {
    log::info!("Message processor host starting");

    let mut processed_count = 0;

    // Process messages until simulation is cancelled
    loop {
        // Check if simulation has been cancelled
        if simvar_harness::utils::is_simulator_cancelled() {
            log::info!("Message processor received cancellation signal");
            break;
        }

        // Simulate processing work
        simvar_harness::switchy::unsync::time::sleep(Duration::from_millis(100)).await;

        processed_count += 1;

        // Log progress periodically
        if processed_count % 10 == 0 {
            log::debug!("Message processor: Processed {processed_count} batches");
        }
    }

    log::info!("Message processor host shutting down - Processed {processed_count} batches total");
    Ok(())
}

/// Client actor that sends numbered messages
async fn run_message_client(client_id: usize, message_interval: Duration) -> ClientResult {
    log::info!("Client {client_id} starting");

    let mut message_count = 0;

    // Send messages until simulation is cancelled
    loop {
        // Check if simulation has been cancelled
        if simvar_harness::utils::is_simulator_cancelled() {
            log::info!("Client {client_id} received cancellation signal");
            break;
        }

        message_count += 1;

        // Simulate sending a message
        log::debug!(
            "Client {client_id}: Sending message #{message_count} at simulation time {}ms",
            simvar_harness::switchy::time::simulator::current_step()
        );

        // Wait before sending next message
        simvar_harness::switchy::unsync::time::sleep(message_interval).await;
    }

    log::info!("Client {client_id} completed - Sent {message_count} messages total");
    Ok(())
}
