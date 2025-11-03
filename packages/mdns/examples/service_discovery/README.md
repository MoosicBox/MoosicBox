# Service Discovery Example

Demonstrates how to discover MoosicBox services on the local network using the mDNS scanner functionality, listening for service announcements and receiving information about discovered services.

## Summary

This example shows how to use the scanner feature to discover MoosicBox services on the network in real-time, using an async channel to receive service information as they are discovered.

## What This Example Demonstrates

- Using the `scanner` feature to discover services on the local network
- Creating and starting the scanner service
- Receiving discovered services through an async channel
- Processing service information (ID, name, host, DNS)
- Graceful shutdown of the scanner service
- Async service lifecycle management

## Prerequisites

- Basic understanding of mDNS/Bonjour service discovery
- Familiarity with async/await in Rust
- Understanding of async channels and `tokio::select!`
- At least one MoosicBox service registered on the network (see `service_registration` example)

## Running the Example

```bash
cargo run --manifest-path packages/mdns/examples/service_discovery/Cargo.toml
```

The scanner will run continuously until you press Ctrl+C.

## Expected Output

When no services are present:

```
=== MoosicBox mDNS Service Discovery Example ===

Scanning the local network for MoosicBox services...
Service type: _moosicboxserver._tcp.local.

Scanner started. Listening for MoosicBox services...
Press Ctrl+C to stop scanning.

```

When services are discovered:

```
=== MoosicBox mDNS Service Discovery Example ===

Scanning the local network for MoosicBox services...
Service type: _moosicboxserver._tcp.local.

Scanner started. Listening for MoosicBox services...
Press Ctrl+C to stop scanning.

DISCOVERED SERVICE:
  ID: MyMusicServer
  Name: hostname.local.
  Host: 192.168.1.100:8000
  DNS: MyMusicServer._moosicboxserver._tcp.local.

DISCOVERED SERVICE:
  ID: AnotherServer
  Name: other-host.local.
  Host: 192.168.1.101:8000
  DNS: AnotherServer._moosicboxserver._tcp.local.
```

When you press Ctrl+C:

```
Shutdown signal received. Stopping scanner...
Scanner stopped.
```

## Code Walkthrough

### 1. Create the Channel

```rust
let (tx, rx) = kanal::unbounded_async::<scanner::MoosicBox>();
```

An unbounded async channel is created to receive discovered services. The scanner will send `MoosicBox` instances through the `tx` sender, and we receive them via `rx`.

### 2. Create Scanner Context

```rust
let ctx = scanner::Context::new(tx);
```

The context holds the scanner's state, including the channel sender for discovered services.

### 3. Create and Start the Scanner Service

```rust
let service = scanner::service::Service::new(ctx).with_name("MoosicBoxScanner");
let handle = service.handle();
let join_handle = service.start();
```

This creates the scanner service and starts it:

1. `Service::new(ctx)` - Creates the service with the provided context
2. `.with_name()` - Sets a descriptive name for logging and task naming
3. `.handle()` - Gets a cloneable handle for sending commands and shutting down
4. `.start()` - Spawns a background task that browses for MoosicBox services and sends discovered services through the channel

### 4. Process Discovered Services

```rust
tokio::select! {
    result = rx.recv() => {
        match result {
            Ok(moosicbox) => {
                // Handle discovered service
                println!("DISCOVERED SERVICE:");
                println!("  ID: {}", moosicbox.id);
                println!("  Name: {}", moosicbox.name);
                println!("  Host: {}", moosicbox.host);
                println!("  DNS: {}", moosicbox.dns);
            }
            Err(e) => {
                eprintln!("Error receiving from scanner: {e}");
                break;
            }
        }
    }
    _ = &mut shutdown_signal => {
        println!("\nShutdown signal received. Stopping scanner...");
        break;
    }
}
```

The `tokio::select!` macro allows us to concurrently:

- Wait for discovered services on the channel
- Listen for the Ctrl+C shutdown signal

### 5. Graceful Shutdown

```rust
handle.shutdown()?;
join_handle.await??;
```

This stops the scanner service:

1. `handle.shutdown()` - Signals the service to stop and cancel the background task
2. `join_handle.await??` - Waits for the service task to complete and propagates any errors

## Key Concepts

### MoosicBox Structure

Each discovered service is represented by a `MoosicBox` struct:

- `id`: Unique identifier extracted from the DNS name (the instance name)
- `name`: DNS hostname of the server (e.g., `hostname.local.`)
- `host`: Socket address (IP and port) where the service is accessible
- `dns`: Full DNS service name (e.g., `MyMusicServer._moosicboxserver._tcp.local.`)

### Scanner Service Architecture

The scanner uses the `moosicbox_async_service` framework:

1. **Context**: Holds state (channel sender, cancellation token, task handle)
2. **Service**: Manages lifecycle (start, shutdown, command processing)
3. **Background Task**: Continuously listens for mDNS service events
4. **Channel Communication**: Sends discovered services to the main task

### Service Events

The scanner listens for `ServiceEvent::ServiceResolved` events from the underlying mDNS daemon. When a service is resolved, it extracts:

- Service information (name, hostname, port)
- IPv4 addresses (filters out IPv6)
- Creates a `MoosicBox` instance with all details

## Testing the Example

### Test with the Service Registration Example

1. In one terminal, run the `service_registration` example:

    ```bash
    cargo run --manifest-path packages/mdns/examples/service_registration/Cargo.toml
    ```

2. In another terminal, run this discovery example:

    ```bash
    cargo run --manifest-path packages/mdns/examples/service_discovery/Cargo.toml
    ```

3. You should see the registered service appear in the discovery output

### Test with Multiple Services

Register multiple services with different instance names and ports, then run the scanner to see them all discovered.

### Test with Real MoosicBox Servers

If you have actual MoosicBox servers running on your network, this scanner will discover them automatically.

## Troubleshooting

### No services discovered

- **Verify services are registered**: Run the `service_registration` example or check for actual MoosicBox servers
- **Check network connectivity**: Ensure devices are on the same local network
- **Firewall issues**: Ensure UDP port 5353 (mDNS) is not blocked
- **Enable logging**: Set `RUST_LOG=debug` to see scanner activity:
    ```bash
    RUST_LOG=debug cargo run --manifest-path packages/mdns/examples/service_discovery/Cargo.toml
    ```

### Scanner fails to start

- **mDNS daemon initialization error**: Check that your system supports mDNS
- **Permission issues**: Some systems may require elevated privileges for mDNS operations

### Services discovered multiple times

This is normal behavior for mDNS. Services may re-announce themselves periodically, and you might see the same service multiple times. In a real application, you would typically:

- Maintain a set of discovered services by ID
- Update existing entries rather than adding duplicates
- Implement a timeout to remove services that haven't been seen recently

## Related Examples

- `service_registration` - Demonstrates registering MoosicBox services that this scanner can discover
