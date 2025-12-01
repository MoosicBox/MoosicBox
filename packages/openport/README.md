# OpenPort - Find a Free Unused Port

A simple, lightweight library for finding available network ports.

## Overview

OpenPort provides a minimal set of functions to:

- Find available ports within a specified range
- Check if specific ports are free on TCP and/or UDP
- Reserve and manage port allocation to prevent conflicts (with `reservation` feature, enabled by default)
- Optionally find random available ports (with `rand` feature)

## Installation

### As a Library

```toml
[dependencies]
openport = { path = "../openport" }

# Or with random port selection feature
# openport = { path = "../openport", features = ["rand"] }
```

## Usage

### Basic Usage

```rust
use openport::pick_unused_port;

fn main() {
    // Find an available port in a specific range
    if let Some(port) = pick_unused_port(15000..16000) {
        println!("Available port: {}", port);
    }

    // Also works with inclusive ranges
    if let Some(port) = pick_unused_port(8000..=9000) {
        println!("Available port: {}", port);
    }
}
```

### Command Line Usage

```bash
# Find any available port (requires 'cli' and 'rand' features)
openport

# Find port in specific range (exclusive) (requires 'cli' feature)
openport 15000 16000

# Find port in inclusive range (requires 'cli' feature)
openport 8000 9000 --inclusive
```

### Random Port Selection (requires `rand` feature)

```rust
use openport::pick_random_unused_port;

fn main() {
    // Find a random available port in range 15000-25000
    if let Some(port) = pick_random_unused_port() {
        println!("Random available port: {}", port);
    }
}
```

### Checking Port Availability

```rust
use openport::{is_free, is_free_tcp, is_free_udp};

fn main() {
    let port = 8080;

    // Check if port is free on both TCP and UDP
    if is_free(port) {
        println!("Port {} is available on both TCP and UDP", port);
    }

    // Check TCP only
    if is_free_tcp(port) {
        println!("Port {} is available on TCP", port);
    }

    // Check UDP only
    if is_free_udp(port) {
        println!("Port {} is available on UDP", port);
    }
}
```

### Port Reservation (requires `reservation` feature, enabled by default)

```rust
use openport::PortReservation;

fn main() {
    // Create a reservation system for a port range
    let reservation = PortReservation::new(15000..16000);

    // Reserve a single port
    if let Some(port) = reservation.reserve_port() {
        println!("Reserved port: {}", port);

        // Check if port is reserved
        assert!(reservation.is_reserved(port));

        // Release the port when done
        reservation.release_port(port);
    }

    // Reserve multiple ports
    let ports = reservation.reserve_ports(5);
    println!("Reserved {} ports", ports.len());

    // Release all ports at once
    reservation.release_ports(ports.iter().copied());
}
```

### Integration with Web Servers

```rust
use openport::pick_unused_port;
use std::net::SocketAddr;

fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    // Find available port
    let port = pick_unused_port(8000..9000)
        .ok_or("No available ports in range")?;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("Server starting on http://{}", addr);

    // Start your server with the port
    Ok(())
}
```

### Testing Utilities

```rust
use openport::pick_unused_port;

#[test]
fn test_with_available_port() {
    // Get a port for testing
    let port = pick_unused_port(10000..20000).unwrap();

    // Use port in test
    let server_addr = format!("127.0.0.1:{}", port);
    // ... start test server and run tests
}
```

## API Reference

### Functions

```rust
/// Find an unused port within the specified range.
/// The port will be available on both TCP and UDP.
/// Returns None if no ports are available in the range.
pub fn pick_unused_port(range: impl PortRange) -> Option<u16>;

/// Find a random unused port in the range 15000-25000.
/// The port will be available on both TCP and UDP.
/// Returns None if no ports are available after several attempts.
/// Requires the `rand` feature.
#[cfg(feature = "rand")]
pub fn pick_random_unused_port() -> Option<u16>;

/// Check if a port is free on both TCP and UDP.
pub fn is_free(port: u16) -> bool;

/// Check if a port is free on TCP.
pub fn is_free_tcp(port: u16) -> bool;

/// Check if a port is free on UDP.
pub fn is_free_udp(port: u16) -> bool;
```

### Types

```rust
/// Port number type alias
pub type Port = u16;

/// Port reservation system for managing port allocation
/// Requires the `reservation` feature (enabled by default)
#[cfg(feature = "reservation")]
pub type PortReservation = reservation::PortReservation<Range<Port>>;

/// Trait for port range types (Range<u16> and RangeInclusive<u16>)
pub trait PortRange {
    fn into_iter(self) -> impl Iterator<Item = u16>;
    fn iter(&self) -> impl Iterator<Item = u16>;
}
```

### PortReservation Methods (requires `reservation` feature)

```rust
impl<R: PortRange> PortReservation<R> {
    /// Creates a new port reservation system with the specified range
    pub const fn new(range: R) -> Self;

    /// Reserve a single port from the range
    pub fn reserve_port(&self) -> Option<Port>;

    /// Reserve multiple ports from the range
    pub fn reserve_ports(&self, num_ports: usize) -> Vec<Port>;

    /// Release a reserved port
    pub fn release_port(&self, port: Port);

    /// Release multiple reserved ports
    pub fn release_ports(&self, ports: impl Iterator<Item = Port>);

    /// Check if a port is currently reserved
    pub fn is_reserved(&self, port: Port) -> bool;
}
```

## Features

- **`reservation`** (default): Enables the `PortReservation` type for managing port allocation and preventing duplicate port usage
- **`cli`**: Enables the command-line binary for finding available ports
- **`rand`**: Enables `pick_random_unused_port()` function for finding random ports in the 15000-25000 range
- **`fail-on-warnings`**: Treats compiler warnings as errors (for development)

## Implementation Details

- Ports are checked by attempting to bind on both IPv4 (`0.0.0.0`) and IPv6 (`[::]`) unspecified addresses
- A port is considered free only if it can be bound on both address families
- For `pick_unused_port()`, ports in the range are checked sequentially until a free one is found
- For `pick_random_unused_port()`, the function tries 10 random ports first, then asks the OS for free ports

## Limitations

- Sequential port checking (no concurrent checks)
- No timeout configuration
- Binding to low ports (<1024) requires root/administrator privileges on most systems
- Race condition: A port that tests as "free" may be taken before you can bind to it (mitigated by using `PortReservation`)

## Examples

### Development Server with Fallback Ports

```rust
use openport::pick_unused_port;

fn start_dev_server() -> Result<(), Box<dyn std::error::Error>> {
    // Try to find a port in the common dev server range
    let port = pick_unused_port(3000..9000)
        .ok_or("No ports available in range 3000-9000")?;

    println!("Development server starting on http://localhost:{}", port);

    // Start your development server
    Ok(())
}
```

## License

openport is provided under the MPL v2.0 license. Please refer to the LICENSE file for more details.
