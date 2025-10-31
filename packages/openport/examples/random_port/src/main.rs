#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Random port selection example for openport
//!
//! This example demonstrates the random port selection feature, which uses
//! the `rand` feature to find available ports in the range 15000-25000.

use openport::{pick_random_unused_port, pick_unused_port};

fn main() {
    println!("=== OpenPort Random Port Example ===\n");

    // Example 1: Using pick_random_unused_port
    println!("1. Finding a random available port (15000-25000 range):");
    match pick_random_unused_port() {
        Some(port) => {
            println!("   ✓ Found random port: {port}");
            println!("   Port is in range: {}", (15000..25000).contains(&port));
        }
        None => println!("   ✗ No available ports found after multiple attempts"),
    }
    println!();

    // Example 2: Comparing random vs sequential selection
    println!("2. Comparing random and sequential port selection:");
    println!("   Finding 5 ports using sequential search (15000..16000):");
    let sequential_ports: Vec<u16> = (0..5)
        .filter_map(|_| pick_unused_port(15000..16000))
        .collect();
    println!("     Sequential ports: {sequential_ports:?}");

    println!("   Finding 5 ports using random search:");
    let random_ports: Vec<u16> = (0..5).filter_map(|_| pick_random_unused_port()).collect();
    println!("     Random ports: {random_ports:?}");
    println!();

    // Example 3: When to use random vs sequential
    println!("3. Use case comparison:");
    println!("   Sequential search (pick_unused_port):");
    println!("     + Predictable - always returns lowest available port");
    println!("     + Fast for small ranges");
    println!("     - May conflict with other processes using low ports");
    println!();
    println!("   Random search (pick_random_unused_port):");
    println!("     + Better distribution across port range");
    println!("     + Less likely to conflict with other services");
    println!("     + Good for production environments");
    println!("     - Slightly slower (tries 10 random attempts first)");
    println!();

    // Example 4: Practical usage - multiple services
    println!("4. Allocating ports for multiple microservices:");
    let services = ["auth-service", "api-gateway", "data-service"];

    for service in services {
        match pick_random_unused_port() {
            Some(port) => {
                println!("   ✓ {service}: http://localhost:{port}");
            }
            None => println!("   ✗ Failed to allocate port for {service}"),
        }
    }
    println!();

    // Example 5: Fallback behavior demonstration
    println!("5. Understanding the fallback behavior:");
    println!("   pick_random_unused_port() tries:");
    println!("   1. Up to 10 random ports in range 15000-25000");
    println!("   2. If none found, asks OS for free TCP port (10 attempts)");
    println!("   3. Verifies OS-provided port is also free on UDP");
    println!("   4. Returns None after 20 total failed attempts");
    println!();

    if let Some(port) = pick_random_unused_port() {
        println!("   ✓ Successfully found port {port} using this algorithm");
    }
    println!();

    println!("=== Example completed successfully ===");
}
