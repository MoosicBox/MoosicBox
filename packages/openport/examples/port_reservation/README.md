# Port Reservation Example

Demonstrates the port reservation system for coordinated port allocation and management across multiple services or components.

## Summary

This example shows how to use `PortReservation` to manage port allocation systematically, preventing conflicts and enabling coordinated port management for multi-service applications.

## What This Example Demonstrates

- Creating a port reservation system with custom ranges
- Reserving single and multiple ports
- Releasing ports back to the pool
- Checking if ports are reserved
- Managing ports for microservices
- Handling port exhaustion gracefully
- Thread-safe port coordination

## Prerequisites

- Understanding of port management basics (see `basic_usage` example)
- Familiarity with microservices or multi-service architectures
- Basic knowledge of Rust ownership and borrowing

## Running the Example

```bash
cargo run --manifest-path packages/openport/examples/port_reservation/Cargo.toml
```

Note: This example requires the `reservation` feature, which is enabled by default in openport.

## Expected Output

```
=== OpenPort Port Reservation Example ===

1. Creating a port reservation system:
   ✓ Created reservation for range 15000..16000

2. Reserving a single port:
   ✓ Reserved port: 15000
   Is reserved: true

3. Reserving multiple ports:
   ✓ Reserved 5 ports: [15001, 15002, 15003, 15004, 15005]
     - Port 15001 is reserved: true
     - Port 15002 is reserved: true
     - Port 15003 is reserved: true
     - Port 15004 is reserved: true
     - Port 15005 is reserved: true

4. Releasing ports:
   Before release: port 15000 reserved = true
   After release: port 15000 reserved = false

5. Releasing multiple ports:
   ✓ Released 5 ports
     - Port 15001 is reserved: false
     - Port 15002 is reserved: false
     - Port 15003 is reserved: false
     - Port 15004 is reserved: false
     - Port 15005 is reserved: false

6. Practical example - managing microservice ports:
   ✓ auth-service: http://localhost:15000
   ✓ api-gateway: http://localhost:15001
   ✓ data-service: http://localhost:15002
   ✓ cache-service: http://localhost:15003

7. Simulating service shutdown:
   Shutting down auth-service...
   ✓ Released port 15000
   Port 15000 is now available for reuse

8. Demonstrating port exhaustion handling:
   ✓ Reservation 1: got port 25000
   ✓ Reservation 2: got port 25001
   ✓ Reservation 3: got port 25002
   ✗ Reservation 4: no ports available
   ✗ Reservation 5: no ports available
   Successfully reserved 3 out of 5 requested ports

=== Example completed successfully ===
```

## Code Walkthrough

### Creating a Reservation System

```rust
use openport::PortReservation;

// Create with custom range (exclusive)
let reservation = PortReservation::new(15000..16000);
```

The reservation system tracks which ports are allocated within the specified range. Note that `PortReservation` is a type alias for `reservation::PortReservation<Range<u16>>`, so it only supports exclusive ranges by default.

### Reserving Ports

```rust
// Reserve a single port
let port = reservation.reserve_port()
    .ok_or("No ports available")?;

// Reserve multiple ports
let ports = reservation.reserve_ports(5); // Returns Vec<u16>
```

Reserved ports are tracked internally and won't be allocated again until released.

### Checking and Releasing Ports

```rust
// Check if a port is reserved
if reservation.is_reserved(port) {
    println!("Port {} is reserved", port);
}

// Release a single port
reservation.release_port(port);

// Release multiple ports
reservation.release_ports(ports.iter().copied());
```

Released ports become available for reservation again.

### Managing Microservices

A practical pattern for service management:

```rust
let manager = PortReservation::new(15000..16000);

struct Service {
    name: String,
    port: u16,
}

let mut services = Vec::new();

// Start services
for name in ["auth", "api", "data"] {
    if let Some(port) = manager.reserve_port() {
        services.push(Service {
            name: name.to_string(),
            port,
        });
        println!("{}: http://localhost:{}", name, port);
    }
}

// Later: stop a service and release its port
if let Some(service) = services.first() {
    manager.release_port(service.port);
    println!("Released port {}", service.port);
}
```

## Key Concepts

### Why Port Reservation?

**Without reservation** (using `pick_unused_port`):

```rust
// Service A
let port_a = pick_unused_port(15000..16000)?; // Gets 15000

// Service B (in same process or different process)
let port_b = pick_unused_port(15000..16000)?; // Might also get 15000!
// Both services try to bind to same port → CONFLICT
```

**With reservation**:

```rust
let manager = PortReservation::new(15000..16000);

// Service A
let port_a = manager.reserve_port()?; // Gets 15000

// Service B
let port_b = manager.reserve_port()?; // Gets 15001 (15000 is tracked as reserved)
// No conflict!
```

### Thread Safety

`PortReservation` uses `Mutex<BTreeSet<Port>>` internally for thread-safe operation:

```rust
use std::sync::Arc;
use std::thread;

let reservation = Arc::new(PortReservation::new(15000..16000));

let handles: Vec<_> = (0..10)
    .map(|_| {
        let res = Arc::clone(&reservation);
        thread::spawn(move || res.reserve_port())
    })
    .collect();

// All threads can safely reserve ports concurrently
for handle in handles {
    if let Some(port) = handle.join().unwrap() {
        println!("Thread reserved port: {}", port);
    }
}
```

### Reservation Tracking

The system maintains a `BTreeSet` of reserved ports:

- `reserve_port()`: Finds first free port, adds to set
- `reserve_ports(n)`: Finds n free ports, adds all to set
- `is_reserved(port)`: Checks if port is in set
- `release_port(port)`: Removes port from set
- `release_ports(iter)`: Removes multiple ports from set

Ports must be both:

1. **Not reserved**: Not in the `BTreeSet`
2. **Actually free**: Can bind on TCP and UDP (checked via `is_free`)

### Memory Overhead

Each reserved port adds one `u16` to the `BTreeSet`:

- 100 reserved ports ≈ 200 bytes + tree overhead
- 1000 reserved ports ≈ 2 KB + tree overhead

Very efficient even for thousands of ports.

### Reservation Lifecycle

```
┌─────────────────┐
│ Port is free    │
│ (not reserved)  │
└────────┬────────┘
         │ reserve_port()
         ▼
┌─────────────────┐
│ Port reserved   │
│ (in BTreeSet)   │
└────────┬────────┘
         │ release_port()
         ▼
┌─────────────────┐
│ Port is free    │
│ (not reserved)  │
└─────────────────┘
```

### Range Types

Both range types are supported:

```rust
// Exclusive range (15000 to 15999)
let res1 = PortReservation::new(15000..16000);

// Inclusive range (15000 to 16000)
let res2 = PortReservation::new(15000..=16000);
```

## Testing the Example

### Verify Reservation Tracking

Run the example and observe that reserved ports are properly tracked:

```bash
cargo run --manifest-path packages/openport/examples/port_reservation/Cargo.toml | \
  grep "is reserved"
```

### Test Exhaustion Handling

The example includes a test with a tiny range (25000..25003):

```rust
let small = PortReservation::new(25000..25003);
let ports = small.reserve_ports(10); // Request more than available
// ports.len() will be ≤ 3 (the actual number available)
```

### Multi-threaded Test

Create a test to verify thread safety:

```rust
use std::sync::Arc;
use std::thread;
use openport::PortReservation;

let reservation = Arc::new(PortReservation::new(15000..16000));
let handles: Vec<_> = (0..10)
    .map(|_| {
        let res = Arc::clone(&reservation);
        thread::spawn(move || {
            res.reserve_ports(10)
        })
    })
    .collect();

let mut all_ports = Vec::new();
for handle in handles {
    all_ports.extend(handle.join().unwrap());
}

// Verify no duplicates
all_ports.sort();
all_ports.dedup();
println!("Reserved {} unique ports", all_ports.len());
```

## Troubleshooting

### "Failed to reserve port" / Returns None

This happens when:

- All ports in the range are either reserved or in use by other processes
- The range is too small for the number of reservations

Solutions:

- Use a larger range: `PortReservation::new(10000..20000)`
- Release unused ports: `reservation.release_port(port)`
- Check system port usage: `netstat -an | wc -l`

### Reserved Port Shows as Not Reserved After Restart

`PortReservation` is **in-memory only**:

```rust
{
    let res = PortReservation::new(15000..16000);
    let port = res.reserve_port().unwrap();
    println!("Reserved: {}", port);
} // res dropped here - all reservations lost!

// Port reservations don't persist
```

For persistent coordination across process restarts, you need external coordination (file locks, Redis, etc.).

### Ports Reserved but Still Can't Bind

`PortReservation` tracks logical reservations but doesn't prevent external processes:

```rust
let res = PortReservation::new(15000..16000);
let port = res.reserve_port().unwrap(); // Says 15000 is reserved

// Meanwhile, external process binds to 15000

// Your bind attempt will fail even though port was "reserved"
```

The reservation system coordinates within your application, not system-wide.

### Memory Leak from Unreleased Ports

If you never release ports, the `BTreeSet` grows indefinitely:

```rust
let res = PortReservation::new(15000..65535);

loop {
    let port = res.reserve_port().unwrap();
    // Never released - memory grows!
}
```

**Solution**: Always release ports when done:

```rust
struct ServiceGuard {
    port: u16,
    reservation: Arc<PortReservation<Range<u16>>>,
}

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        self.reservation.release_port(self.port);
        println!("Auto-released port {}", self.port);
    }
}
```

### Cross-Process Coordination

`PortReservation` only works within a single process. For multiple processes:

**Option 1**: Shared reservation instance (via IPC)

**Option 2**: External coordination service (Redis, etcd, etc.)

**Option 3**: Non-overlapping ranges per process:

```rust
// Process A
let res_a = PortReservation::new(15000..20000);

// Process B
let res_b = PortReservation::new(20000..25000);

// No conflicts possible
```

## Related Examples

- `basic_usage` - Introduction to core openport functions
- `random_port` - Random port selection for better distribution
