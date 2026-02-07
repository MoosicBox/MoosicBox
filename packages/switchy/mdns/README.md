# switchy_mdns

mDNS service registration and discovery for MoosicBox servers.

## Features

- **Service Registration**: Register MoosicBox servers on the local network via mDNS
- **Simulator**: Simulated mDNS daemon for testing (requires `simulator` feature)

## Cargo Features

| Feature     | Default | Description                                  |
| ----------- | ------- | -------------------------------------------- |
| `simulator` | Yes     | Provides a simulated mDNS daemon for testing |

## Usage

### Registering a Service

```rust,no_run
use switchy_mdns::register_service;

async fn example() -> Result<(), switchy_mdns::RegisterServiceError> {
    register_service("my-server", "192.168.1.100", 8080).await?;
    Ok(())
}
```

## License

This project is licensed under the MPL-2.0 License.
