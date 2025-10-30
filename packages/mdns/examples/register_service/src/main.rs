#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating mDNS service registration.
//!
//! This example shows how to register a `MoosicBox` service on the local network
//! using mDNS/Bonjour protocol, making it discoverable by other devices.

use switchy_mdns::{RegisterServiceError, register_service};

#[tokio::main]
async fn main() -> Result<(), RegisterServiceError> {
    // Initialize logging to see what's happening
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("=== MoosicBox mDNS Service Registration Example ===\n");

    // Define service parameters
    let instance_name = "MyMusicServer";
    let ip_address = "192.168.1.100";
    let port = 8000;

    println!("Registering MoosicBox service:");
    println!("  Instance Name: {instance_name}");
    println!("  IP Address: {ip_address}");
    println!("  Port: {port}");
    println!("  Service Type: {}\n", switchy_mdns::SERVICE_TYPE);

    // Register the service on the local network
    // This makes the service discoverable by other devices using mDNS/Bonjour
    register_service(instance_name, ip_address, port).await?;

    println!("✓ Service registered successfully!");
    println!("\nThe service is now discoverable on the local network.");
    println!("Other devices can find it using mDNS/Bonjour service discovery.");
    println!("\nPress Ctrl+C to unregister and exit...\n");

    // Keep the service running until interrupted
    // The service will be automatically unregistered when the program exits
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    println!("\nShutting down and unregistering service...");

    Ok(())
}
