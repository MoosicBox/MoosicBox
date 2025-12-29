# Switchy P2P

P2P communication abstraction system - enables direct device-to-device connections.

## Features

### Core Abstractions

- **Trait-based P2P System**: Generic abstractions over different P2P implementations
    - `P2PSystem` - Core system interface for connections and discovery
    - `P2PConnection` - Reliable message streaming between peers
    - `P2PNodeId` - 256-bit node identifiers with deterministic/random generation

### Network Simulator

The package includes a complete P2P network simulator with realistic network conditions:

- **Topology Simulation**: Graph-based network with configurable links and nodes
- **Network Conditions**: Configurable latency, packet loss, and bandwidth limits
- **Discovery System**: DNS-like peer discovery with name registration
- **Network Partitions**: Support for simulating and healing network splits
- **Async Message Passing**: FIFO-ordered message delivery with realistic delays
- **Environment Configuration**: Tunable parameters via environment variables:
    - `SIMULATOR_DEFAULT_LATENCY_MS` (default: 50ms)
    - `SIMULATOR_DEFAULT_PACKET_LOSS` (default: 1%)
    - `SIMULATOR_DISCOVERY_DELAY_MS` (default: 100ms)
    - `SIMULATOR_CONNECTION_TIMEOUT_SECS` (default: 30s)
    - `SIMULATOR_MAX_MESSAGE_SIZE` (default: 1MB)

### Usage Example

```rust
use switchy_p2p::simulator::SimulatorP2P;

// Create a P2P node with deterministic ID for testing
let node = SimulatorP2P::with_seed("alice");
let node_id = node.local_node_id().clone();

println!("Node ID: {}", node_id.fmt_short());
```

The simulator provides methods for peer discovery and connection:

- `register_peer(name, node_id)` - Register a name for discovery
- `discover(name)` - Look up a node ID by name
- `connect(node_id)` - Connect to a peer by node ID
- `connect_by_name(name)` - Discover and connect in one step

Connections support `send()` and `recv()` for async message passing.

## Cargo Features

- `default` = `["simulator"]` - Enables the network simulator
- `simulator` - Network simulation implementation
- `fail-on-warnings` - Treat warnings as errors (CI use)

**Planned:**

- `iroh` - Integration with Iroh for production P2P networking
- `test-utils` - Testing utilities and property-based tests

## Dependencies

- `switchy_random` - Deterministic and random number generation
- `switchy_async` - Async runtime utilities (sync, time, tokio)
- `async-trait` - Async trait support
- `thiserror` - Error type definitions

## Architecture

### Module Structure

- `types` - Core error types (`P2PError`, `P2PResult`)
- `traits` - Generic P2P abstractions (`P2PSystem`, `P2PConnection`, `P2PNodeId`)
- `simulator` - Complete network simulation implementation

### Future Roadmap

**Planned features:**

- Iroh integration for production use with real NAT traversal
- Connection listener support (`P2PListener` trait)
- Property-based testing with proptest
- Advanced routing algorithms beyond BFS
- Bandwidth throttling simulation

## License

Licensed under the same terms as the parent project.
