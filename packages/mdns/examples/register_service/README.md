# Service Registration Example

This example demonstrates how to register a MoosicBox service on the local network using mDNS (multicast DNS) service discovery.

## Summary

This example shows how to use `switchy_mdns` to register a MoosicBox service instance on your local network, making it discoverable by other devices using mDNS/Bonjour protocols.

## What This Example Demonstrates

- Registering a MoosicBox service with a custom instance name
- Specifying IP address and port for the service
- Using the standard MoosicBox service type constant
- Handling service registration errors
- Keeping the service active until interrupted
- Automatic service unregistration on program exit

## Prerequisites

- Basic understanding of mDNS/Bonjour service discovery
- Knowledge of async Rust and Tokio runtime
- A network interface to register the service on
- Understanding of IP addresses and port numbers

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/mdns/examples/register_service/Cargo.toml
```

The service will remain active until you press Ctrl+C.

## Expected Output

When you run the example, you should see output similar to:

```
=== MoosicBox mDNS Service Registration Example ===

Registering MoosicBox service:
  Instance Name: MyMusicServer
  IP Address: 192.168.1.100
  Port: 8000
  Service Type: _moosicboxserver._tcp.local.

[DEBUG] register_service: Registering mdns service service_type=_moosicboxserver._tcp.local. instance_name=MyMusicServer host_name=my-hostname.local. ip=192.168.1.100 port=8000
[DEBUG] register_service: Registered mdns service service_type=_moosicboxserver._tcp.local. instance_name=MyMusicServer host_name=my-hostname.local. ip=192.168.1.100 port=8000
✓ Service registered successfully!

The service is now discoverable on the local network.
Other devices can find it using mDNS/Bonjour service discovery.

Press Ctrl+C to unregister and exit...
```

## Code Walkthrough

### Service Parameters

```rust
let instance_name = "MyMusicServer";
let ip_address = "192.168.1.100";
let port = 8000;
```

Define the service parameters:

- `instance_name`: A human-readable name for this service instance
- `ip_address`: The IP address where the service is accessible
- `port`: The TCP port number where the service listens

### Service Registration

```rust
register_service(instance_name, ip_address, port).await?;
```

The `register_service` function registers the service with the mDNS daemon:

1. Gets the system hostname automatically
2. Creates a `ServiceInfo` object with the standard MoosicBox service type
3. Registers the service on the local network
4. Makes the service discoverable by mDNS/Bonjour clients

### Keeping the Service Active

```rust
tokio::signal::ctrl_c()
    .await
    .expect("Failed to listen for Ctrl+C");
```

The service must remain active to be discoverable. The example waits for Ctrl+C before exiting. When the program exits, the service is automatically unregistered.

## Key Concepts

### mDNS Service Discovery

mDNS (multicast DNS) is a protocol that allows devices on a local network to discover services without requiring a central DNS server. It's the foundation of Apple's Bonjour and is supported on most platforms.

### Service Type

The `SERVICE_TYPE` constant (`_moosicboxserver._tcp.local.`) follows the DNS-SD (DNS Service Discovery) naming convention:

- `_moosicboxserver`: The service name
- `_tcp`: The protocol (TCP)
- `local.`: The domain for local network services

### Hostname Resolution

The library automatically detects the system hostname and appends `.local.` to create the fully qualified hostname used in service registration.

### Error Handling

The `RegisterServiceError` enum provides two error variants:

- `MdnsSd`: Errors from the underlying mDNS service daemon
- `IO`: I/O errors when getting the hostname

## Testing the Example

### Using macOS Discovery Tools

On macOS, you can verify the service is registered using the `dns-sd` command:

```bash
dns-sd -B _moosicboxserver._tcp local.
```

This will browse for MoosicBox services on the local network.

### Using Avahi (Linux)

On Linux with Avahi installed:

```bash
avahi-browse -r _moosicboxserver._tcp
```

### Programmatic Discovery

You can also test discovery programmatically using the companion `discover_services` example.

## Troubleshooting

### Service Not Appearing in Discovery

- Ensure mDNS/Bonjour is enabled on your network
- Check that firewalls aren't blocking UDP port 5353 (mDNS port)
- Verify the IP address is valid for your network interface
- Try using `0.0.0.0` to bind to all available interfaces

### Permission Errors

On some systems, binding to certain ports or multicast groups may require elevated privileges. Try running with appropriate permissions if you encounter permission errors.

### Hostname Resolution Fails

If the hostname cannot be determined, the library falls back to "unknown". Ensure your system hostname is properly configured.

## Related Examples

- **discover_services**: Demonstrates scanning for and discovering MoosicBox services on the network (requires the `scanner` feature)
