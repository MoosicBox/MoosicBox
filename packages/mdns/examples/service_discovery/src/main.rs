#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating mDNS service discovery.
//!
//! This example shows how to discover `MoosicBox` services on the local network
//! using the mDNS scanner functionality.

use switchy_mdns::scanner::{self, service::Commander};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see scanner activity
    env_logger::init();

    println!("=== MoosicBox mDNS Service Discovery Example ===\n");
    println!("Scanning the local network for MoosicBox services...");
    println!("Service type: {}\n", switchy_mdns::SERVICE_TYPE);

    // Create a channel to receive discovered services
    // The scanner will send MoosicBox instances through this channel
    let (tx, rx) = kanal::unbounded_async::<scanner::MoosicBox>();

    // Create the scanner context with the channel sender
    let ctx = scanner::Context::new(tx);

    // Create and start the scanner service
    // This spawns a background task that listens for mDNS service announcements
    let service = scanner::service::Service::new(ctx).with_name("MoosicBoxScanner");
    let handle = service.handle();
    let join_handle = service.start();

    println!("Scanner started. Listening for MoosicBox services...");
    println!("Press Ctrl+C to stop scanning.\n");

    // Set up Ctrl+C handler to gracefully shutdown
    let mut shutdown_signal = tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");
    });

    // Process discovered services as they arrive
    loop {
        tokio::select! {
            // Handle discovered services
            result = rx.recv() => {
                match result {
                    Ok(moosicbox) => {
                        // A new MoosicBox service was discovered
                        println!("DISCOVERED SERVICE:");
                        println!("  ID: {}", moosicbox.id);
                        println!("  Name: {}", moosicbox.name);
                        println!("  Host: {}", moosicbox.host);
                        println!("  DNS: {}", moosicbox.dns);
                        println!();
                    }
                    Err(e) => {
                        eprintln!("Error receiving from scanner: {e}");
                        break;
                    }
                }
            }
            // Handle Ctrl+C shutdown
            _ = &mut shutdown_signal => {
                println!("\nShutdown signal received. Stopping scanner...");
                break;
            }
        }
    }

    // Stop the scanner service
    handle.shutdown()?;

    // Wait for the service to complete
    join_handle.await??;

    println!("Scanner stopped.");

    Ok(())
}
