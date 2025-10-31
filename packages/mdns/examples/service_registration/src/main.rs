#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating mDNS service registration.
//!
//! This example shows how to register a `MoosicBox` service on the local network
//! using the mDNS protocol, making it discoverable by other devices.

use switchy_mdns::RegisterServiceError;

#[tokio::main]
async fn main() -> Result<(), RegisterServiceError> {
    // Initialize logging to see what's happening
    env_logger::init();

    println!("=== MoosicBox mDNS Service Registration Example ===\n");

    // Service registration parameters
    let instance_name = "MyMusicServer";
    let ip_address = "192.168.1.100";
    let port = 8000;

    println!("Registering MoosicBox service with the following parameters:");
    println!("  Instance name: {instance_name}");
    println!("  IP address: {ip_address}");
    println!("  Port: {port}");
    println!("  Service type: {}\n", switchy_mdns::SERVICE_TYPE);

    // Register the service on the local network
    // This makes the service discoverable via mDNS/Bonjour
    switchy_mdns::register_service(instance_name, ip_address, port).await?;

    println!("SUCCESS: MoosicBox service registered successfully!");
    println!("\nThe service is now discoverable on the local network.");
    println!("Other devices can discover this service using mDNS/Bonjour.");
    println!("\nPress Ctrl+C to stop the service and exit.\n");

    // Keep the service running until interrupted
    // The service will remain registered until this program exits
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    println!("\nShutting down service...");

    Ok(())
}
