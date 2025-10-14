# Switchy mDNS

Simple multicast DNS (mDNS) service registration library for the Switchy ecosystem, providing basic service announcement capabilities on local networks using the Zeroconf/Bonjour protocol.

## Features

- **Service Registration**: Register MoosicBox services on the local network
- **mDNS Service Discovery**: Basic mDNS service announcement support
- **Service Browsing**: Optional MoosicBox service discovery/browsing capabilities (via `scanner` feature)
- **Error Handling**: Error handling for service registration and scanning
- **Hostname Detection**: Automatic hostname detection for service registration
- **Simulator Support**: Optional simulator mode for testing without real mDNS

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
# Default includes scanner and simulator features
switchy_mdns = "0.1.4"

# Or customize features
switchy_mdns = { version = "0.1.4", default-features = false }
```

## Usage

### Service Registration

```rust
use switchy_mdns::{register_service, RegisterServiceError};

#[tokio::main]
async fn main() -> Result<(), RegisterServiceError> {
    // Register a MoosicBox service on the network
    register_service(
        "MyMusicServer",    // instance name
        "192.168.1.100",    // IP address
        8000,               // port
    ).await?;

    println!("MoosicBox service registered successfully!");

    // Keep the service running
    tokio::signal::ctrl_c().await.unwrap();

    Ok(())
}
```

### Service Type

The library uses a standard service type for MoosicBox servers:

```rust
use switchy_mdns::SERVICE_TYPE;

println!("Service type: {}", SERVICE_TYPE); // "_moosicboxserver._tcp.local."
```

### Error Handling

```rust
use switchy_mdns::RegisterServiceError;

match register_service("MyServer", "192.168.1.100", 8000).await {
    Ok(()) => println!("Service registered successfully"),
    Err(RegisterServiceError::MdnsSd(e)) => {
        eprintln!("mDNS error: {}", e);
    }
    Err(RegisterServiceError::IO(e)) => {
        eprintln!("I/O error: {}", e);
    }
}
```

## Cargo Features

- **Default**: Includes both `scanner` and `simulator` features
- **scanner**: MoosicBox service discovery/browsing capabilities
- **simulator**: Use simulator instead of real mDNS for testing

## Dependencies

Core dependencies:
- `mdns-sd`: Core mDNS service daemon functionality
- `hostname`: System hostname detection
- `thiserror`: Error handling utilities
- `async-trait`: Async trait support
- `log`: Logging functionality
- `moosicbox_assert`: Assertion utilities

Additional dependencies for `scanner` feature:
- `kanal`: Async channel for service discovery events
- `moosicbox_async_service`: Async service framework
- `strum_macros`: Enum utilities
- `switchy_async`: Async runtime utilities

## Error Types

- `RegisterServiceError`: Wraps mDNS and I/O errors during service registration

The library provides a simple interface for announcing MoosicBox services on local networks, making them discoverable by other devices using standard mDNS/Bonjour protocols.
