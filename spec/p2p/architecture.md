# P2P Architecture

## System Overview

The P2P integration provides a peer-to-peer communication alternative to MoosicBox's existing centralized tunnel server architecture. The system uses a trait-based abstraction that allows for multiple implementations while maintaining a consistent API.

```
Current Architecture (Tunnel):
Client A ←→ Tunnel Server ←→ Client B

Proposed Architecture (P2P):
Client A ←----------→ Client B
        (Direct P2P)
```

## Design Goals

### Primary Objectives
- **Direct Connections**: Enable direct peer-to-peer communication without central infrastructure
- **NAT Traversal**: Automatic hole-punching and relay fallback for complex network configurations
- **Performance**: Reduce latency and improve throughput compared to tunnel approach
- **Reliability**: Maintain or improve connection reliability through P2P redundancy

### Secondary Objectives
- **Implementation Flexibility**: Support multiple P2P libraries through trait abstraction
- **Testing**: Provide deterministic simulation for reliable automated testing
- **Migration**: Smooth transition path from existing tunnel infrastructure
- **Compatibility**: Maintain backward compatibility during migration period

## Component Architecture

### Core Abstractions

```rust
trait P2PProvider: Send + Sync {
    type Connection: P2PConnection;
    type Listener: P2PListener;

    async fn connect(&self, peer_id: &str) -> Result<Self::Connection>;
    async fn listen(&self, addr: &str) -> Result<Self::Listener>;
    fn local_peer_id(&self) -> &str;
}

trait P2PConnection: Send + Sync {
    async fn send(&self, data: &[u8]) -> Result<()>;
    async fn recv(&self) -> Result<Vec<u8>>;
    fn peer_id(&self) -> &str;
    fn is_connected(&self) -> bool;
}

trait P2PListener: Send + Sync {
    async fn accept(&mut self) -> Result<Box<dyn P2PConnection>>;
    fn local_addr(&self) -> &str;
}
```

### Implementation Hierarchy

```
moosicbox_p2p
├── Core Traits (always available)
│   ├── P2PProvider
│   ├── P2PConnection
│   └── P2PListener
├── Common Types
│   ├── P2PError
│   ├── PeerInfo
│   └── ConnectionConfig
├── Simulator Implementation (feature = "simulator")
│   ├── SimulatorP2P
│   ├── SimulatorConnection
│   └── SimulatorListener
└── Iroh Implementation (feature = "iroh")
    ├── IrohP2P
    ├── IrohConnection
    └── IrohListener
```

## Implementation Details

### Simulator Implementation

**Purpose**: Deterministic testing and development without real network dependencies

**Architecture**:
- In-memory message routing between simulated peers
- Configurable network conditions (latency, packet loss, failures)
- Thread-safe peer registry for connection establishment
- Environment variable configuration following switchy patterns

**Key Features**:
```rust
// Environment variable configuration
SIMULATOR_P2P_LATENCY_MS=50       // Simulated network latency
SIMULATOR_P2P_PACKET_LOSS=0.01    // 1% packet loss rate
SIMULATOR_P2P_FAILURE_RATE=0.001  // Connection failure rate
```

**Message Flow**:
```
Peer A → SimulatorP2P → MessageQueue → SimulatorP2P → Peer B
                      ↑                ↓
                  [Latency]      [Packet Loss]
                  [Failures]     [Bandwidth]
```

### Iroh Implementation

**Purpose**: Production P2P communication with real networking

**Architecture**:
- Wraps Iroh's Endpoint and Connection types
- QUIC-based transport with automatic encryption
- Integrated NAT traversal using STUN/TURN
- Public key based peer identity

**Key Features**:
- **Automatic NAT Traversal**: Hole-punching with relay fallback
- **Connection Persistence**: Automatic reconnection on network changes
- **Multiple Transports**: UDP, TCP, and relay connections
- **Security**: Built-in encryption and authentication

**Iroh Integration**:
```rust
// Iroh Endpoint wrapping
struct IrohP2P {
    endpoint: iroh::Endpoint,
    config: P2PConfig,
}

// Connection wrapping
struct IrohConnection {
    connection: iroh::Connection,
    peer_id: String,
}
```

## Connection Lifecycle

### Connection Establishment

```mermaid
sequenceDiagram
    participant A as Peer A
    participant B as Peer B

    A->>A: provider.connect(peer_b_id)
    A->>B: Connection Request
    B->>B: listener.accept()
    B->>A: Connection Accept
    A->>B: Handshake
    B->>A: Handshake Response
    Note over A,B: Connection Established
    A<->B: Data Exchange
```

### Message Flow

1. **Connection Request**: Peer A initiates connection to Peer B using peer ID
2. **Discovery**: P2P implementation locates Peer B (via DHT, direct address, etc.)
3. **NAT Traversal**: Automatic hole-punching or relay connection
4. **Handshake**: Protocol negotiation and authentication
5. **Data Transfer**: Bidirectional message exchange
6. **Connection Management**: Keep-alive, reconnection, cleanup

### Error Handling

```rust
enum P2PError {
    ConnectionFailed(String),
    PeerNotFound(String),
    NetworkError(NetworkError),
    ProtocolError(String),
    TimeoutError,
    AuthenticationFailed,
}
```

## Protocol Design

### Message Format

```rust
// Basic message structure
struct P2PMessage {
    message_id: u64,
    peer_id: String,
    payload: Vec<u8>,
    timestamp: SystemTime,
}

// Protocol messages
enum ProtocolMessage {
    Handshake { version: u32, capabilities: Vec<String> },
    Data { payload: Vec<u8> },
    Keepalive,
    Disconnect { reason: String },
}
```

### Protocol Versioning

- Version negotiation during handshake
- Backward compatibility for migration period
- Feature capability advertisement
- Graceful degradation for unsupported features

## Security Model

### Identity and Authentication

- **Peer Identity**: Public key based identification
- **Authentication**: Challenge-response using peer's private key
- **Connection Security**: All data encrypted in transit (QUIC provides this)

### Network Security

- **Encrypted Transport**: QUIC provides built-in encryption
- **Peer Verification**: Authenticate peer identity before data exchange
- **DoS Protection**: Rate limiting and connection limits
- **Network Isolation**: Proper firewall and network segmentation

## Performance Considerations

### Connection Management

- **Connection Pooling**: Reuse connections for multiple requests
- **Keep-alive**: Maintain connections across requests
- **Lazy Connection**: Connect on first use, not initialization
- **Connection Limits**: Prevent resource exhaustion

### Message Optimization

- **Message Batching**: Combine small messages for efficiency
- **Compression**: Optional compression for large payloads
- **Streaming**: Support for large data transfers
- **Prioritization**: Priority queues for different message types

### Resource Management

- **Memory**: Bounded buffers and connection limits
- **CPU**: Efficient serialization and async processing
- **Network**: Bandwidth management and QoS
- **File Descriptors**: Proper cleanup and limits

## Network Topology Considerations

### Direct Connections

```
Peer A ←→ Peer B
```
- Lowest latency
- Best bandwidth utilization
- Requires successful NAT traversal

### Relay Connections

```
Peer A ←→ Relay Server ←→ Peer B
```
- Fallback when direct connection fails
- Higher latency but more reliable
- Uses Iroh's relay infrastructure

### Hybrid Topology

```
Peer A ←→ Peer B (direct)
Peer A ←→ Relay ←→ Peer C (relayed)
```
- Combination based on network conditions
- Automatic fallback and optimization
- Dynamic topology adjustment

## Integration Strategy

### Feature Flag Control

```rust
// Compile-time selection
#[cfg(feature = "p2p-simulator")]
let provider = moosicbox_p2p::SimulatorP2P::new(config);

#[cfg(feature = "p2p-iroh")]
let provider = moosicbox_p2p::IrohP2P::new(config);
```

### Configuration Management

```toml
# Environment configuration
P2P_IMPLEMENTATION=iroh
P2P_LISTEN_PORT=8800
P2P_BOOTSTRAP_PEERS=peer1,peer2
P2P_RELAY_SERVERS=relay1.example.com,relay2.example.com
```

### Migration Path

1. **Phase 1**: Deploy P2P alongside tunnel (feature flags)
2. **Phase 2**: Route new connections through P2P
3. **Phase 3**: Migrate existing connections gradually
4. **Phase 4**: Deprecate tunnel infrastructure

## Monitoring and Observability

### Metrics Collection

- **Connection Metrics**: Establishment time, success rate, active connections
- **Performance Metrics**: Latency, throughput, error rates
- **Network Metrics**: NAT traversal success, relay usage
- **Resource Metrics**: Memory usage, CPU utilization

### Logging Integration

```rust
// Structured logging
log::info!("P2P connection established";
    "peer_id" => peer_id,
    "connection_type" => "direct",
    "latency_ms" => establishment_time.as_millis()
);
```

### Health Checks

- Connection pool health
- Peer reachability tests
- Network connectivity validation
- Performance threshold monitoring

## Testing Strategy

### Unit Testing

- Trait-based tests work with all implementations
- Mock implementations for isolated testing
- Error condition and edge case coverage
- Performance and resource usage validation

### Integration Testing

- Real network scenarios with Iroh
- Simulator testing for deterministic scenarios
- Cross-implementation compatibility
- Migration and compatibility testing

### Performance Testing

- Latency and throughput benchmarks
- Scalability testing with multiple peers
- Resource usage profiling
- Comparison with tunnel baseline

## Deployment Considerations

### Infrastructure Requirements

- **Simulator**: No external dependencies
- **Iroh**: Internet connectivity for NAT traversal
- **Relay Servers**: Optional relay infrastructure for fallback

### Configuration Management

- Environment variable configuration
- Runtime configuration updates where safe
- Secure configuration for production
- Development vs production settings

### Rollout Strategy

- Feature flags for gradual enablement
- Canary deployments with monitoring
- Rollback procedures for issues
- Performance monitoring during rollout

## Future Enhancements

### Additional P2P Libraries

The trait-based architecture allows for additional implementations:
- libp2p integration
- Custom protocol implementations
- Specialized network optimizations

### Advanced Features

- **Mesh Networking**: Multi-hop routing for complex topologies
- **Content Distribution**: Efficient data distribution across peers
- **Load Balancing**: Distribute load across multiple peer connections
- **Caching**: Peer-based caching for improved performance

### Protocol Evolution

- Version negotiation framework
- Feature capability system
- Backward compatibility maintenance
- Migration tools for protocol upgrades
