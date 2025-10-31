# Service Registration Example

Demonstrates how to register a MoosicBox service on the local network using mDNS (Multicast DNS), making it discoverable by other devices using Bonjour/Zeroconf protocols.

## Summary

This example shows the basic workflow for registering an mDNS service, including setting up the service parameters, calling the registration function, and keeping the service alive.

## What This Example Demonstrates

- Registering an mDNS service with instance name, IP address, and port
- Using the `register_service` function for service announcement
- Understanding the service type constant (`SERVICE_TYPE`)
- Keeping a service registered until the program exits
- Basic error handling for registration failures

## Prerequisites

- Basic understanding of mDNS/Bonjour service discovery
- Familiarity with async/await in Rust
- Network access (the example uses a local IP address)

## Running the Example

```bash
cargo run --manifest-path packages/mdns/examples/service_registration/Cargo.toml
```

The service will run until you press Ctrl+C.

## Expected Output

```
=== MoosicBox mDNS Service Registration Example ===

Registering MoosicBox service with the following parameters:
  Instance name: MyMusicServer
  IP address: 192.168.1.100
  Port: 8000
  Service type: _moosicboxserver._tcp.local.

SUCCESS: MoosicBox service registered successfully!

The service is now discoverable on the local network.
Other devices can discover this service using mDNS/Bonjour.

Press Ctrl+C to stop the service and exit.
```

When you press Ctrl+C, you'll see:

```
Shutting down service...
```

## Code Walkthrough

### 1. Service Parameters

```rust
let instance_name = "MyMusicServer";
let ip_address = "192.168.1.100";
let port = 8000;
```

These define the service instance:

- `instance_name`: Human-readable name for your service
- `ip_address`: IP address where the service is accessible
- `port`: Port number the service listens on

### 2. Service Registration

```rust
switchy_mdns::register_service(instance_name, ip_address, port).await?;
```

This single function call registers the service on the network. The service will be announced using the standard MoosicBox service type (`_moosicboxserver._tcp.local.`).

### 3. Keeping the Service Alive

```rust
tokio::signal::ctrl_c()
    .await
    .expect("Failed to listen for Ctrl+C");
```

The service remains registered as long as the program runs. This waits for Ctrl+C before exiting.

## Key Concepts

### mDNS Service Registration

mDNS (Multicast DNS) allows services to advertise themselves on a local network without requiring a central DNS server. When you register a service:

1. The service information is broadcast to the local network
2. Other devices can discover the service by browsing for the service type
3. The service remains advertised until the program exits or explicitly unregisters

### Service Types

The `SERVICE_TYPE` constant (`_moosicboxserver._tcp.local.`) follows the DNS-SD naming convention:

- `_moosicboxserver`: The service name
- `_tcp`: The protocol (TCP)
- `local.`: The domain (local network)

### Error Handling

The `register_service` function returns `Result<(), RegisterServiceError>` which can fail due to:

- `MdnsSd` errors: Issues with the underlying mDNS daemon
- `IO` errors: Problems reading the system hostname

## Testing the Example

### Using macOS/iOS (Bonjour Browser)

1. Run the example
2. Open Safari and navigate to "Bonjour" in the bookmarks sidebar
3. Look for "MyMusicServer" in the list of discovered services

### Using Linux (avahi-browse)

```bash
# Install avahi-browse if needed
sudo apt-get install avahi-utils

# Browse for MoosicBox services
avahi-browse -r _moosicboxserver._tcp
```

### Using the Service Discovery Example

If available, run the `service_discovery` example in another terminal to discover this registered service.

## Troubleshooting

### Service not appearing in browsers

- **Check firewall settings**: Ensure UDP port 5353 (mDNS) is not blocked
- **Verify network interface**: The service broadcasts on all available network interfaces
- **Check IP address**: Ensure the IP address matches your actual network interface

### Registration fails with IO error

- The error likely occurs when reading the system hostname
- Check that your system hostname is properly configured: `hostname`

### Registration fails with MdnsSd error

- Another mDNS service might be running on the same port
- Check that the mDNS daemon can be initialized on your system

## Related Examples

- `service_discovery` - Demonstrates discovering MoosicBox services on the network (if available)
