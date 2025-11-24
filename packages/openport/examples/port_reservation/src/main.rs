#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Port reservation example for openport
//!
//! This example demonstrates the port reservation system, which allows
//! managing and coordinating port allocation across multiple services.

use openport::PortReservation;

struct Service {
    name: &'static str,
    port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OpenPort Port Reservation Example ===\n");

    // Example 1: Basic reservation usage
    println!("1. Creating a port reservation system:");
    let reservation = PortReservation::new(15000..16000);
    println!("   ✓ Created reservation for range 15000..16000");
    println!();

    // Example 2: Reserving a single port
    println!("2. Reserving a single port:");
    let port1 = reservation.reserve_port().ok_or("Failed to reserve port")?;
    println!("   ✓ Reserved port: {port1}");
    println!("   Is reserved: {}", reservation.is_reserved(port1));
    println!();

    // Example 3: Reserving multiple ports
    println!("3. Reserving multiple ports:");
    let reserved_ports = reservation.reserve_ports(5);
    println!(
        "   ✓ Reserved {} ports: {:?}",
        reserved_ports.len(),
        reserved_ports
    );
    for port in &reserved_ports {
        println!(
            "     - Port {port} is reserved: {}",
            reservation.is_reserved(*port)
        );
    }
    println!();

    // Example 4: Releasing ports
    println!("4. Releasing ports:");
    println!(
        "   Before release: port {port1} reserved = {}",
        reservation.is_reserved(port1)
    );
    reservation.release_port(port1);
    println!(
        "   After release: port {port1} reserved = {}",
        reservation.is_reserved(port1)
    );
    println!();

    // Example 5: Releasing multiple ports
    println!("5. Releasing multiple ports:");
    reservation.release_ports(reserved_ports.iter().copied());
    println!("   ✓ Released {} ports", reserved_ports.len());
    for port in &reserved_ports {
        println!(
            "     - Port {port} is reserved: {}",
            reservation.is_reserved(*port)
        );
    }
    println!();

    // Example 6: Practical usage - managing microservices
    println!("6. Practical example - managing microservice ports:");
    let service_manager = PortReservation::new(15000..16000);

    let mut services = Vec::new();

    // Allocate ports for multiple services
    for service_name in [
        "auth-service",
        "api-gateway",
        "data-service",
        "cache-service",
    ] {
        if let Some(port) = service_manager.reserve_port() {
            println!("   ✓ {service_name}: http://localhost:{port}");
            services.push(Service {
                name: service_name,
                port,
            });
        }
    }
    println!();

    // Simulate service shutdown
    println!("7. Simulating service shutdown:");
    if let Some(service) = services.first() {
        println!("   Shutting down {}...", service.name);
        service_manager.release_port(service.port);
        println!("   ✓ Released port {}", service.port);
        println!("   Port {} is now available for reuse", service.port);
    }
    println!();

    // Example 8: Handling exhaustion
    println!("8. Demonstrating port exhaustion handling:");
    let small_reservation = PortReservation::new(25000..25003);
    let mut all_ports = Vec::new();

    // Try to reserve more ports than available
    for i in 1..=5 {
        if let Some(port) = small_reservation.reserve_port() {
            println!("   ✓ Reservation {i}: got port {port}");
            all_ports.push(port);
        } else {
            println!("   ✗ Reservation {i}: no ports available");
        }
    }
    println!(
        "   Successfully reserved {} out of 5 requested ports",
        all_ports.len()
    );
    println!();

    println!("=== Example completed successfully ===");

    Ok(())
}
