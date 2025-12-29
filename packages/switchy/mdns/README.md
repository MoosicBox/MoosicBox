# switchy_mdns

mDNS service registration and discovery for MoosicBox servers.

## Features

- **Service Registration**: Register MoosicBox servers on the local network via mDNS
- **Service Discovery**: Discover MoosicBox servers on the local network (requires `scanner`
  feature)
- **Simulator**: Simulated mDNS daemon for testing (requires `simulator` feature)

## Cargo Features

| Feature     | Default | Description                                        |
| ----------- | ------- | -------------------------------------------------- |
| `scanner`   | Yes     | Enables mDNS service discovery for finding servers |
| `simulator` | Yes     | Provides a simulated mDNS daemon for testing       |

## Usage

### Registering a Service

```rust,no_run
use switchy_mdns::register_service;

async fn example() -> Result<(), switchy_mdns::RegisterServiceError> {
    register_service("my-server", "192.168.1.100", 8080).await?;
    Ok(())
}
```

### Discovering Servers

With the `scanner` feature enabled, you can discover MoosicBox servers on the network:

```rust,ignore
use switchy_mdns::scanner::{Context, MoosicBox, service::Service};

// Create a channel to receive discovered servers
let (tx, rx) = kanal::unbounded_async::<MoosicBox>();

// Create the scanner context and service
let context = Context::new(tx);
// Start the service to begin scanning...

// Discovered servers will be sent through the channel
while let Ok(server) = rx.recv().await {
    println!("Found server: {} at {}", server.name, server.host);
}
```

## License

This project is licensed under the MPL-2.0 License.
