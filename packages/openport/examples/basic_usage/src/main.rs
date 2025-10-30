#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for the openport crate.
//!
//! This example demonstrates the core functionality of finding available ports
//! using different methods and checking port availability.

use std::net::{TcpListener, UdpSocket};

fn main() {
    println!("=== OpenPort Basic Usage Example ===\n");

    // Example 1: Find an available port in a specific range
    println!("1. Finding a port in range 8000..9000:");
    match openport::pick_unused_port(8000..9000) {
        Some(port) => {
            println!("   Found available port: {port}");
            println!(
                "   Port is in expected range: {}",
                (8000..9000).contains(&port)
            );
        }
        None => println!("   No available ports found in range"),
    }
    println!();

    // Example 2: Using inclusive ranges
    println!("2. Finding a port in inclusive range 15000..=16000:");
    match openport::pick_unused_port(15000..=16000) {
        Some(port) => {
            println!("   Found available port: {port}");
            println!(
                "   Port is in expected range: {}",
                (15000..=16000).contains(&port)
            );
        }
        None => println!("   No available ports found in range"),
    }
    println!();

    // Example 3: Check if specific ports are available
    println!("3. Checking specific port availability:");
    let test_port = 8080;
    println!("   Checking port {test_port}");
    println!("   - TCP available: {}", openport::is_free_tcp(test_port));
    println!("   - UDP available: {}", openport::is_free_udp(test_port));
    println!("   - Both available: {}", openport::is_free(test_port));
    println!();

    // Example 4: Demonstrate that a bound port is not free
    println!("4. Demonstrating port detection after binding:");
    if let Some(port) = openport::pick_unused_port(20000..21000) {
        println!("   Found free port: {port}");
        println!(
            "   Port is free before binding: {}",
            openport::is_free(port)
        );

        // Bind to the port
        let _listener =
            TcpListener::bind(format!("127.0.0.1:{port}")).expect("Failed to bind to port");
        println!("   Bound TCP listener to port {port}");
        println!(
            "   Port is free after TCP binding: {}",
            openport::is_free_tcp(port)
        );

        // Note: The listener will be dropped at the end of this block, freeing the port
    }
    println!();

    // Example 5: Find a port and actually use it
    println!("5. Finding a port and creating a simple server:");
    if let Some(port) = openport::pick_unused_port(30000..31000) {
        println!("   Starting server on port {port}");

        // Create a TCP listener
        match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(listener) => {
                println!("   ✓ TCP server successfully bound to 127.0.0.1:{port}");
                println!("   Server address: {}", listener.local_addr().unwrap());
            }
            Err(e) => println!("   ✗ Failed to bind: {e}"),
        }

        // Create a UDP socket on the same port
        match UdpSocket::bind(format!("127.0.0.1:{port}")) {
            Ok(socket) => {
                println!("   ✓ UDP socket successfully bound to 127.0.0.1:{port}");
                println!("   Socket address: {}", socket.local_addr().unwrap());
            }
            Err(e) => println!("   ✗ Failed to bind UDP: {e}"),
        }
    }
    println!();

    // Example 6: Finding multiple ports
    println!("6. Finding multiple available ports:");
    let mut ports = Vec::new();
    let range = 40000..41000;

    for port in range.clone() {
        if openport::is_free(port) {
            ports.push(port);
            if ports.len() >= 5 {
                break;
            }
        }
    }

    let start = range.start;
    let end = range.end;
    println!("   Found {} available ports in range {start}..{end}:", ports.len());
    for (i, port) in ports.iter().enumerate() {
        let num = i + 1;
        println!("     {num}. Port {port}");
    }
    println!();

    println!("=== Example Complete ===");
}
