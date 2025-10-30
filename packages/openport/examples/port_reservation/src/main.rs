#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Port reservation example for the openport crate.
//!
//! This example demonstrates the port reservation system, which provides
//! thread-safe management of port allocations to prevent conflicts.

use std::{net::TcpListener, sync::Arc, thread, time::Duration};

use openport::PortReservation;

fn main() {
    println!("=== OpenPort Reservation System Example ===\n");

    // Example 1: Basic reservation and release
    basic_reservation_example();
    println!();

    // Example 2: Reserving multiple ports
    multiple_ports_example();
    println!();

    // Example 3: Thread-safe concurrent reservations
    concurrent_reservations_example();
    println!();

    // Example 4: Using reserved ports with actual servers
    server_integration_example();
    println!();

    // Example 5: Default reservation system
    default_reservation_example();
    println!();

    println!("=== Example Complete ===");
}

/// Demonstrates basic port reservation and release
fn basic_reservation_example() {
    println!("1. Basic Port Reservation:");

    // Create a reservation system for ports in range 15000..16000
    let reservation = PortReservation::new(15000..16000);

    // Reserve a port
    match reservation.reserve_port() {
        Some(port) => {
            println!("   Reserved port: {port}");
            println!("   Port is reserved: {}", reservation.is_reserved(port));

            // Release the port
            reservation.release_port(port);
            println!("   Released port: {port}");
            println!(
                "   Port is reserved after release: {}",
                reservation.is_reserved(port)
            );
        }
        None => println!("   No available ports found"),
    }
}

/// Demonstrates reserving multiple ports at once
fn multiple_ports_example() {
    println!("2. Reserving Multiple Ports:");

    let reservation = PortReservation::new(16000..17000);

    // Reserve 5 ports at once
    let ports = reservation.reserve_ports(5);
    println!("   Reserved {} ports:", ports.len());

    for (i, port) in ports.iter().enumerate() {
        let num = i + 1;
        println!(
            "     {num}. Port {port} (reserved: {})",
            reservation.is_reserved(*port)
        );
    }

    // Release all ports at once
    reservation.release_ports(ports.iter().copied());
    println!("   Released all ports");

    // Verify they're released
    let any_still_reserved = ports.iter().any(|&port| reservation.is_reserved(port));
    println!("   Any ports still reserved: {any_still_reserved}");
}

/// Demonstrates thread-safe concurrent port reservations
fn concurrent_reservations_example() {
    println!("3. Thread-Safe Concurrent Reservations:");

    // Create a shared reservation system
    let reservation = Arc::new(PortReservation::new(17000..18000));
    let mut handles = Vec::new();

    println!("   Spawning 5 threads to reserve ports concurrently...");

    // Spawn multiple threads that each reserve a port
    for i in 1..=5 {
        let reservation_clone = Arc::clone(&reservation);
        let handle = thread::spawn(move || {
            // Simulate some work
            thread::sleep(Duration::from_millis(10));

            // Reserve a port
            reservation_clone.reserve_port().map_or_else(
                || {
                    println!("     Thread {i} failed to reserve a port");
                    0
                },
                |port| {
                    println!("     Thread {i} reserved port: {port}");
                    port
                },
            )
        });
        handles.push(handle);
    }

    // Collect all reserved ports
    let mut reserved_ports = Vec::new();
    for handle in handles {
        let port = handle.join().expect("Thread panicked");
        if port != 0 {
            reserved_ports.push(port);
        }
    }

    println!("   Total ports reserved: {}", reserved_ports.len());

    // Verify all ports are unique
    let mut sorted_ports = reserved_ports.clone();
    sorted_ports.sort_unstable();
    sorted_ports.dedup();
    println!(
        "   All reserved ports are unique: {}",
        sorted_ports.len() == reserved_ports.len()
    );

    // Clean up
    reservation.release_ports(reserved_ports.into_iter());
    println!("   Released all reserved ports");
}

/// Demonstrates using reserved ports with actual TCP servers
fn server_integration_example() {
    println!("4. Server Integration with Reservations:");

    let reservation = PortReservation::new(18000..19000);

    // Reserve ports for multiple services
    let ports = reservation.reserve_ports(3);

    if ports.len() >= 3 {
        println!("   Reserved {} ports for services:", ports.len());

        // Start "services" on the reserved ports
        let mut listeners = Vec::new();

        for (i, &port) in ports.iter().enumerate() {
            let service_name = match i {
                0 => "Web Server",
                1 => "API Server",
                2 => "Admin Panel",
                _ => "Service",
            };

            match TcpListener::bind(format!("127.0.0.1:{port}")) {
                Ok(listener) => {
                    println!("   ✓ {service_name} started on port {port}");
                    listeners.push(listener);
                }
                Err(e) => {
                    println!("   ✗ Failed to start {service_name} on port {port}: {e}");
                }
            }
        }

        // Keep listeners alive to demonstrate the ports are in use
        println!("   All services running on reserved ports");

        // Verify ports are both reserved and bound
        for &port in &ports {
            println!(
                "     Port {port} - Reserved: {}, Can bind again: {}",
                reservation.is_reserved(port),
                openport::is_free_tcp(port)
            );
        }

        // Drop listeners (ports become available again)
        drop(listeners);
        println!("   Stopped all services");

        // Release reservations
        reservation.release_ports(ports.into_iter());
        println!("   Released all port reservations");
    } else {
        println!("   Not enough ports available for demonstration");
    }
}

/// Demonstrates the default reservation system
fn default_reservation_example() {
    println!("5. Default Reservation System:");

    // Use the default range (15000..65535)
    // PortReservation is a type alias for PortReservation<Range<u16>>
    let reservation = PortReservation::default();

    if let Some(port) = reservation.reserve_port() {
        println!("   Reserved port from default range: {port}");
        println!(
            "   Port is in default range (15000..65535): {}",
            (15000..65535).contains(&port)
        );

        reservation.release_port(port);
        println!("   Released port");
    } else {
        println!("   No available ports in default range");
    }
}
