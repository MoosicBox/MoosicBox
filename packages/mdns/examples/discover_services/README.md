# Service Discovery Example

This example demonstrates how to discover MoosicBox services on the local network using mDNS (multicast DNS) service scanning.

## Summary

This example shows how to use the `scanner` feature of `switchy_mdns` to continuously scan the local network for MoosicBox service instances, receiving discovered services through an async channel.

## What This Example Demonstrates

- Starting the mDNS scanner service to discover MoosicBox servers
- Receiving discovered services through an async channel
- Processing service information (ID, name, host address, DNS name)
- Handling the scanner lifecycle (start and shutdown)
- Gracefully stopping the scanner on Ctrl+C

## Prerequisites

- Basic understanding of mDNS/Bonjour service discovery
- Knowledge of async Rust and Tokio runtime
- Understanding of async channels (using the `kanal` crate)
- A network with MoosicBox services to discover (or run the `register_service` example)

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/mdns/examples/discover_services/Cargo.toml
```

The scanner will run continuously until you press Ctrl+C.

## Expected Output

When you run the example on a network with MoosicBox services, you should see output similar to:

```
=== MoosicBox mDNS Service Discovery Example ===

Scanning the local network for MoosicBox services...
Service Type: _moosicboxserver._tcp.local.
Press Ctrl+C to stop scanning.

[DEBUG] mdns scanner: Browsing for _moosicboxserver._tcp.local. services...
Scanner started. Listening for MoosicBox services...

[DEBUG] mdns scanner: Found server instance: MyMusicServer._moosicboxserver._tcp.local.
[DEBUG] mdns scanner: Server address: 192.168.1.100
=== Discovered Server #1 ===
  ID:   MyMusicServer
  Name: my-hostname.local.
  Host: 192.168.1.100:8000
  DNS:  MyMusicServer._moosicboxserver._tcp.local.

^C
Shutting down scanner...

Scanner stopped.
Total services discovered: 1
```

## Code Walkthrough

### Creating the Channel

```rust
let (tx, rx) = kanal::unbounded_async::<switchy_mdns::scanner::MoosicBox>();
```

Create an unbounded async channel for receiving discovered services. The scanner sends `MoosicBox` instances through this channel whenever a service is discovered.

### Setting Up the Scanner

```rust
let context = Context::new(tx);
let mut scanner = Service::new(context)?;
scanner.start().await?;
```

The scanner is initialized with a context containing the sender side of the channel. Starting the scanner spawns a background task that continuously monitors the network for MoosicBox services.

### Processing Discovered Services

```rust
loop {
    tokio::select! {
        result = rx.recv() => {
            match result {
                Ok(server) => {
                    println!("=== Discovered Server #{} ===", discovered_count);
                    println!("  ID:   {}", server.id);
                    println!("  Name: {}", server.name);
                    println!("  Host: {}", server.host);
                    println!("  DNS:  {}", server.dns);
                }
                // ...
            }
        }
        _ = tokio::signal::ctrl_c() => {
            break;
        }
    }
}
```

The example uses `tokio::select!` to:

- Receive discovered services from the channel as they arrive
- Handle Ctrl+C for graceful shutdown

Each discovered service provides:

- `id`: Unique identifier for the server instance
- `name`: Human-readable hostname
- `host`: Socket address (IP:port)
- `dns`: Full DNS service name

### Graceful Shutdown

```rust
scanner.shutdown().await?;
```

When exiting, the scanner is properly shut down to clean up resources and stop the background task.

## Key Concepts

### mDNS Service Scanning

The scanner uses the mDNS protocol to continuously browse for services of type `_moosicboxserver._tcp.local.` on the local network. When services announce themselves or respond to queries, they are resolved and sent through the channel.

### Async Service Pattern

The scanner follows the async service pattern from `moosicbox_async_service`:

1. **Context**: Holds the state and configuration for the service
2. **Service**: Manages the lifecycle (start, shutdown, command processing)
3. **Background Task**: Runs continuously in the background performing the scanning

### Channel Communication

The scanner uses an unbounded async channel to communicate discovered services:

- **Unbounded**: Can hold unlimited messages (bounded channels can block senders)
- **Async**: Compatible with async/await and Tokio runtime
- **One-way**: Scanner sends, application receives

### Service Resolution

When the scanner detects a service announcement:

1. The `ServiceEvent::ServiceResolved` event provides full service information
2. The scanner extracts IPv4 addresses and creates socket addresses
3. A `MoosicBox` struct is created with all relevant information
4. The struct is sent through the channel to the application

## Testing the Example

### Test with the Registration Example

The easiest way to test discovery is to run the `register_service` example in another terminal:

```bash
# Terminal 1: Run the discovery scanner
cargo run --manifest-path packages/mdns/examples/discover_services/Cargo.toml

# Terminal 2: Register a service
cargo run --manifest-path packages/mdns/examples/register_service/Cargo.toml
```

You should see the registered service appear in the discovery output.

### Test with Multiple Services

Run multiple instances of `register_service` with different instance names to see the scanner discover multiple services:

```bash
# Modify the instance_name in register_service/src/main.rs before running
# each instance, or create a parameterized version
```

### Test with System Tools

You can also register services using system tools and discover them with this example:

- **macOS**: Use `dns-sd -R "TestServer" _moosicboxserver._tcp local 8000`
- **Linux**: Use `avahi-publish -s "TestServer" _moosicboxserver._tcp 8000`

## Troubleshooting

### No Services Discovered

- Ensure there are MoosicBox services running on the network
- Check that mDNS/Bonjour is enabled on your network
- Verify firewalls aren't blocking UDP port 5353 (mDNS port)
- Try running the `register_service` example to create a test service

### Scanner Errors

If the scanner fails to start:

- Ensure you have network permissions to perform multicast operations
- Check that the `scanner` feature is enabled in dependencies
- Review debug logs for detailed error information

### Channel Errors

If you see channel receive errors, the sender side may have been dropped unexpectedly. This usually indicates the scanner background task has terminated due to an error.

## Related Examples

- **register_service**: Demonstrates registering a MoosicBox service on the network for this scanner to discover
