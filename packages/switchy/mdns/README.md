# switchy_mdns

mDNS service registration for MoosicBox servers.

## Features

- **Service Registration**: Register MoosicBox servers on the local network via mDNS
- **Simulator**: Simulated mDNS daemon for testing (enabled by default)

## Cargo Features

| Feature            | Default | Description                                                           |
| ------------------ | ------- | --------------------------------------------------------------------- |
| `simulator`        | Yes     | Provides a simulated mDNS daemon for testing                          |
| `fail-on-warnings` | No      | Enables strict warning handling through the `moosicbox_assert` crate |

## Installation

```toml
[dependencies]
switchy_mdns = "0.1.4"

# Or disable default features (disables the simulator)
switchy_mdns = { version = "0.1.4", default-features = false }
```

## Usage

### Registering a Service

```rust,no_run
use switchy_mdns::register_service;

async fn example() -> Result<(), switchy_mdns::RegisterServiceError> {
    register_service("my-server", "192.168.1.100", 8080).await?;
    Ok(())
}
```

## Public API

- `register_service(instance_name, ip, port)`: Registers a MoosicBox mDNS service instance
- `SERVICE_TYPE`: The service type constant used for registration (`_moosicboxserver._tcp.local.`)
- `RegisterServiceError`: Error type returned by `register_service`
  - `RegisterServiceError::MdnsSd`: Underlying `mdns_sd` daemon or registration error
  - `RegisterServiceError::IO`: Hostname lookup I/O error
- `switchy_mdns::service::MdnsServiceDaemon`: Trait abstraction for service daemon implementations
- `switchy_mdns::service::MdnsSdServiceDaemon`: Wrapper for real `mdns_sd::ServiceDaemon`
- `switchy_mdns::service::simulator::SimulatorServiceDaemon`: Simulator daemon (available with `simulator` feature)

## License

This project is licensed under the MPL-2.0 License.
