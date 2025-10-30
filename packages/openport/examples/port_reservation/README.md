# Port Reservation Example

A comprehensive example demonstrating the port reservation system in the `openport` crate for thread-safe port management.

## Summary

This example shows how to use the `PortReservation` system to reserve and manage ports in a thread-safe manner, preventing conflicts when multiple parts of your application need to allocate ports simultaneously.

## What This Example Demonstrates

- Creating a port reservation system with custom ranges
- Reserving single and multiple ports
- Releasing reserved ports
- Thread-safe concurrent port reservations
- Integration with actual TCP servers
- Using the default reservation system
- Verifying reservation state

## Prerequisites

- Understanding of Rust's ownership and borrowing
- Basic knowledge of threading and `Arc` (Atomic Reference Counting)
- Familiarity with network ports and TCP

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/openport/examples/port_reservation/Cargo.toml
```

Or from the example directory:

```bash
cd packages/openport/examples/port_reservation
cargo run
```

## Expected Output

```
=== OpenPort Reservation System Example ===

1. Basic Port Reservation:
   Reserved port: 15000
   Port is reserved: true
   Released port: 15000
   Port is reserved after release: false

2. Reserving Multiple Ports:
   Reserved 5 ports:
     1. Port 16000 (reserved: true)
     2. Port 16001 (reserved: true)
     3. Port 16002 (reserved: true)
     4. Port 16003 (reserved: true)
     5. Port 16004 (reserved: true)
   Released all ports
   Any ports still reserved: false

3. Thread-Safe Concurrent Reservations:
   Spawning 5 threads to reserve ports concurrently...
     Thread 1 reserved port: 17000
     Thread 2 reserved port: 17001
     Thread 3 reserved port: 17002
     Thread 4 reserved port: 17003
     Thread 5 reserved port: 17004
   Total ports reserved: 5
   All reserved ports are unique: true
   Released all reserved ports

4. Server Integration with Reservations:
   Reserved 3 ports for services:
   ✓ Web Server started on port 18000
   ✓ API Server started on port 18001
   ✓ Admin Panel started on port 18002
   All services running on reserved ports
     Port 18000 - Reserved: true, Can bind again: false
     Port 18001 - Reserved: true, Can bind again: false
     Port 18002 - Reserved: true, Can bind again: false
   Stopped all services
   Released all port reservations

5. Default Reservation System:
   Reserved port from default range: 15000
   Port is in default range (15000..65535): true
   Released port

=== Example Complete ===
```

Note: Actual port numbers will vary depending on what ports are available on your system.

## Code Walkthrough

### 1. Creating a Reservation System

Create a `PortReservation` instance with your desired port range:

```rust
use openport::PortReservation;

// Exclusive range
let reservation = PortReservation::new(15000..16000);

// Inclusive range
let reservation = PortReservation::new(15000..=16000);

// Default range (15000..65535)
// Note: PortReservation is a type alias and doesn't need generics
let reservation = PortReservation::default();
```

### 2. Reserving and Releasing Single Ports

Reserve a single port and later release it:

```rust
// Reserve a port
let port = reservation.reserve_port().expect("No ports available");
println!("Reserved port: {}", port);

// Check if it's reserved
assert!(reservation.is_reserved(port));

// Release the port
reservation.release_port(port);
assert!(!reservation.is_reserved(port));
```

### 3. Reserving Multiple Ports

Reserve multiple ports at once:

```rust
// Reserve 5 ports
let ports = reservation.reserve_ports(5);
println!("Reserved {} ports", ports.len());

// All ports are automatically reserved
for port in &ports {
    assert!(reservation.is_reserved(*port));
}

// Release all at once
reservation.release_ports(ports.iter().copied());
```

### 4. Thread-Safe Concurrent Access

The reservation system is thread-safe via internal `Mutex`:

```rust
use std::sync::Arc;
use std::thread;

// Share reservation across threads
let reservation = Arc::new(PortReservation::new(17000..18000));

let mut handles = Vec::new();
for i in 1..=5 {
    let reservation_clone = Arc::clone(&reservation);
    let handle = thread::spawn(move || {
        // Each thread safely reserves its own port
        reservation_clone.reserve_port()
    });
    handles.push(handle);
}

// Collect results
for handle in handles {
    if let Some(port) = handle.join().unwrap() {
        println!("Thread reserved port: {}", port);
    }
}
```

All reservations are guaranteed to be unique - no two threads will receive the same port.

### 5. Integration with Servers

Reserve ports before starting servers to avoid conflicts:

```rust
use std::net::TcpListener;

let reservation = PortReservation::new(18000..19000);
let ports = reservation.reserve_ports(3);

// Start servers on reserved ports
let web_port = ports[0];
let api_port = ports[1];
let admin_port = ports[2];

let web_server = TcpListener::bind(format!("127.0.0.1:{}", web_port))?;
let api_server = TcpListener::bind(format!("127.0.0.1:{}", api_port))?;
let admin_server = TcpListener::bind(format!("127.0.0.1:{}", admin_port))?;

println!("Web: {}, API: {}, Admin: {}", web_port, api_port, admin_port);

// When done, release reservations
reservation.release_ports(ports.into_iter());
```

## Key Concepts

### Thread Safety

`PortReservation` uses a `Mutex<BTreeSet<Port>>` internally to ensure thread-safe access. Multiple threads can:

- Reserve ports concurrently without conflicts
- Check reservation status safely
- Release ports from any thread

The mutex ensures that only one thread can modify the reservation state at a time.

### Reservation vs. System Availability

The reservation system tracks two states:

1. **Reservation state**: Whether your application has marked the port as reserved
2. **System availability**: Whether the OS reports the port as bindable

A port is only allocated if **both** conditions are met:

- Not already reserved in the system
- Actually free on the OS (can be bound)

```rust
// Port must pass both checks to be reserved
fn is_free(reserved_set: &BTreeSet<Port>, port: Port) -> bool {
    !reserved_set.contains(&port) && openport::is_free(port)
}
```

### Preventing Double Allocation

The reservation system prevents the same port from being allocated twice:

```rust
let port = reservation.reserve_port().unwrap();
assert!(reservation.is_reserved(port));

// This will NOT return the same port
let another_port = reservation.reserve_port().unwrap();
assert_ne!(port, another_port);
```

### When to Use Reservations

Use the reservation system when:

- **Multiple services**: Starting multiple servers that need different ports
- **Concurrent operations**: Multiple threads/tasks need to allocate ports
- **Long-lived allocations**: Need to "claim" a port before actually binding to it
- **Testing**: Allocating multiple ports for test servers without conflicts

### When NOT to Use Reservations

Skip the reservation system if:

- Single-threaded application with sequential port allocation
- Immediately binding after finding a free port (use `pick_unused_port` instead)
- No risk of concurrent port allocation

## Testing the Example

The example is fully self-contained and demonstrates:

1. Basic reserve/release operations work correctly
2. Multiple ports can be reserved without conflicts
3. Concurrent threads receive unique ports
4. Reserved ports can be used with actual TCP servers
5. Default reservation ranges work as expected

Run the example and verify:

- All reserved ports are within specified ranges
- No duplicate ports are allocated
- Reservation state correctly reflects reserve/release operations
- Thread-safe concurrent access works without panics

## Troubleshooting

### "No ports available"

This means all ports in your specified range are either:

- Already reserved in your reservation system
- In use by other processes on the system

**Solution**:

- Use a larger range (e.g., `15000..25000`)
- Release previously reserved ports
- Choose a different port range

### Ports still showing as "not free" after release

The reservation system only tracks **your application's reservations**, not what's actually bound on the system.

When you see:

```rust
reservation.release_port(port);
assert!(!reservation.is_reserved(port));  // true - reservation cleared
assert!(openport::is_free(port));         // may be false if something bound to it
```

**Explanation**: If you bound a server to the port, releasing the reservation doesn't unbind the server. You must drop the `TcpListener` or `UdpSocket` to free the OS-level binding.

### Mutex poisoning panics

If a thread panics while holding the reservation lock, the mutex becomes "poisoned" and subsequent operations will panic.

**Solution**:

- Ensure your code doesn't panic while holding reservations
- Use `std::panic::catch_unwind` if necessary
- Consider implementing a custom error handling strategy

## Related Examples

- **basic_usage**: Demonstrates core port-finding functionality without reservations
