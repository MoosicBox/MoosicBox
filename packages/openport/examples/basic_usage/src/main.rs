#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for openport
//!
//! This example demonstrates the core functionality of the openport library,
//! including finding available ports in a range and checking port availability.

use openport::{is_free, is_free_tcp, is_free_udp, pick_unused_port};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OpenPort Basic Usage Example ===\n");

    // Example 1: Find an available port in a specific range (exclusive)
    println!("1. Finding available port in range 15000..16000 (exclusive):");
    match pick_unused_port(15000..16000) {
        Some(port) => println!("   ✓ Found available port: {port}"),
        None => println!("   ✗ No available ports in range"),
    }
    println!();

    // Example 2: Find an available port using inclusive range
    println!("2. Finding available port in range 8000..=9000 (inclusive):");
    match pick_unused_port(8000..=9000) {
        Some(port) => println!("   ✓ Found available port: {port}"),
        None => println!("   ✗ No available ports in range"),
    }
    println!();

    // Example 3: Check if specific ports are free
    println!("3. Checking availability of specific ports:");
    let test_ports = [8080, 3000, 5000, 9090];
    for port in test_ports {
        if is_free(port) {
            println!("   ✓ Port {port} is free on both TCP and UDP");
        } else {
            println!("   ✗ Port {port} is in use");
        }
    }
    println!();

    // Example 4: Check TCP and UDP separately
    println!("4. Checking TCP and UDP availability separately:");
    let port = 8080;
    let tcp_free = is_free_tcp(port);
    let udp_free = is_free_udp(port);
    println!("   Port {port}:");
    println!("     TCP: {}", if tcp_free { "free" } else { "in use" });
    println!("     UDP: {}", if udp_free { "free" } else { "in use" });
    println!();

    // Example 5: Practical usage - finding a port for a service
    println!("5. Practical example - allocating port for a web server:");
    let port = pick_unused_port(3000..9000)
        .ok_or("No available ports in common web server range (3000-9000)")?;
    println!("   ✓ Allocated port {port} for web server");
    println!("   Server would start at: http://localhost:{port}");
    println!();

    // Example 6: Finding multiple ports
    println!("6. Finding multiple available ports:");
    let mut ports = Vec::new();
    for _ in 0..3 {
        if let Some(port) = pick_unused_port(10000..20000) {
            // Check that we didn't already find this port
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    println!("   Found {} ports: {:?}", ports.len(), ports);
    println!();

    println!("=== Example completed successfully ===");

    Ok(())
}
