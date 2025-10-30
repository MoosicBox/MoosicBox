#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating mDNS service discovery.
//!
//! This example shows how to scan the local network for `MoosicBox` services
//! using the scanner feature of `switchy_mdns`.

use switchy_mdns::scanner::{
    Context,
    service::{Commander, Service},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see what's happening
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("=== MoosicBox mDNS Service Discovery Example ===\n");
    println!("Scanning the local network for MoosicBox services...");
    println!("Service Type: {}", switchy_mdns::SERVICE_TYPE);
    println!("Press Ctrl+C to stop scanning.\n");

    // Create a channel to receive discovered services
    // The scanner will send discovered MoosicBox instances through this channel
    let (tx, rx) = kanal::unbounded_async::<switchy_mdns::scanner::MoosicBox>();

    // Create the scanner context with the sender channel
    let context = Context::new(tx);

    // Create and start the scanner service
    // This spawns a background task that continuously scans for MoosicBox services
    let service = Service::new(context);
    let handle = service.handle();
    let _join_handle = service.start();

    println!("Scanner started. Listening for MoosicBox services...\n");

    // Process discovered services as they arrive
    let mut discovered_count = 0;

    loop {
        tokio::select! {
            // Wait for discovered services from the scanner
            result = rx.recv() => {
                match result {
                    Ok(server) => {
                        discovered_count += 1;
                        println!("=== Discovered Server #{discovered_count} ===");
                        println!("  ID:   {}", server.id);
                        println!("  Name: {}", server.name);
                        println!("  Host: {}", server.host);
                        println!("  DNS:  {}", server.dns);
                        println!();
                    }
                    Err(e) => {
                        eprintln!("Error receiving from channel: {e}");
                        break;
                    }
                }
            }

            // Handle Ctrl+C to gracefully shutdown
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down scanner...");
                break;
            }
        }
    }

    // Shutdown the scanner service
    handle.shutdown()?;

    println!("\nScanner stopped.");
    println!("Total services discovered: {discovered_count}");

    Ok(())
}
