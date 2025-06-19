# MoosicBox mDNS

Simple multicast DNS (mDNS) service registration library for the MoosicBox ecosystem, providing basic service announcement capabilities on local networks using the Zeroconf/Bonjour protocol.

## Features

- **Service Registration**: Register MoosicBox services on the local network
- **mDNS Service Discovery**: Basic mDNS service announcement support
- **Network Scanning**: Optional network device scanning capabilities
- **Error Handling**: Basic error handling for service registration
- **Hostname Detection**: Automatic hostname detection for service registration

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_mdns = "0.1.1"

# Enable scanner feature for network discovery
moosicbox_mdns = { version = "0.1.1", features = ["scanner"] }
```

## Usage

### Service Registration

```rust
use moosicbox_mdns::{register_service, RegisterServiceError};

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
use moosicbox_mdns::SERVICE_TYPE;

println!("Service type: {}", SERVICE_TYPE); // "_moosicboxserver._tcp.local."
```

### Error Handling

```rust
use moosicbox_mdns::RegisterServiceError;

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

## Features

- **Default**: Basic service registration functionality
- **scanner**: Additional network scanning capabilities
- **simulator**: Use simulator instead of real mDNS for testing

## Dependencies

- `mdns-sd`: Core mDNS service daemon functionality
- `hostname`: System hostname detection
- `thiserror`: Error handling utilities

## Error Types

- `RegisterServiceError`: Wraps mDNS and I/O errors during service registration

The library provides a simple interface for announcing MoosicBox services on local networks, making them discoverable by other devices using standard mDNS/Bonjour protocols.
