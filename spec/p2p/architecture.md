# P2P Architecture

## System Overview

The P2P integration provides a peer-to-peer communication alternative to MoosicBox's existing centralized tunnel server architecture. The system uses a trait-based abstraction with zero-cost abstractions that allows for multiple implementations while maintaining a consistent API.

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
- **Zero-Cost Abstractions**: No runtime overhead when using production Iroh backend

### Secondary Objectives
- **Implementation Flexibility**: Support multiple P2P libraries through trait abstraction
- **Deterministic Testing**: Provide controllable simulation for reliable automated testing
- **Migration**: Clean transition path from existing tunnel infrastructure
- **Web-Server-Like API**: Familiar REST-like routing abstraction for integration

## Component Architecture

### Core Abstractions with Zero-Cost Design

```rust
// Main system trait with associated types for zero-cost abstraction
trait P2PSystem: Send + Sync + 'static {
    type NodeId: P2PNodeId;
    type Connection: P2PConnection<NodeId = Self::NodeId>;
    type Listener: P2PListener<Connection = Self::Connection>;

    async fn connect(&self, node_id: Self::NodeId) -> Result<Self::Connection, P2PError>;
    async fn listen(&self, addr: &str) -> Result<Self::Listener, P2PError>;
    fn local_node_id(&self) -> &Self::NodeId;
}

// Node identity trait matching Iroh's approach
trait P2PNodeId: Clone + Debug + Display + Send + Sync + 'static {
    fn from_bytes(bytes: &[u8; 32]) -> Result<Self, P2PError>;
    fn as_bytes(&self) -> &[u8; 32];
    fn fmt_short(&self) -> String;
}

// Connection trait for reliable streams (initial implementation)
trait P2PConnection: Send + Sync + 'static {
    type NodeId: P2PNodeId;

    async fn send(&mut self, data: &[u8]) -> Result<(), P2PError>;
    async fn recv(&mut self) -> Result<Vec<u8>, P2PError>;
    fn remote_node_id(&self) -> &Self::NodeId;
    fn is_connected(&self) -> bool;
    fn close(&mut self) -> Result<(), P2PError>;
}

// Listener trait for accepting connections
trait P2PListener: Send + Sync + 'static {
    type Connection: P2PConnection;

    async fn accept(&mut self) -> Result<Self::Connection, P2PError>;
    fn local_addr(&self) -> &str;
}
```

### Implementation Hierarchy

```
packages/p2p/
├── Cargo.toml                  # Features: simulator (default), iroh
├── src/
│   ├── lib.rs                  # Public API and trait definitions
│   ├── types.rs                # P2PError and common types
│   ├── simulator.rs            # Simulator implementation (feature = "simulator")
│   ├── iroh.rs                 # Iroh implementation (feature = "iroh")
│   └── test_utils.rs           # Testing utilities and helpers
├── tests/                      # Integration tests
│   ├── simulator_tests.rs
│   ├── cross_implementation.rs
│   └── property_tests.rs
├── examples/                   # Usage examples
│   ├── basic_communication.rs
│   ├── service_routing.rs
│   └── migration_example.rs
└── benches/                    # Performance benchmarks
    ├── connection_latency.rs
    └── throughput.rs
```

### Feature Configuration

```toml
[features]
default = ["simulator"]
simulator = [
    "dep:switchy_async",
    "dep:switchy_time",
    "dep:switchy_random",
    "dep:proptest"
]
iroh = [
    "dep:iroh",
    "dep:iroh-net",
    "dep:tokio"
]
fail-on-warnings = []
```

### Zero-Cost Backend Selection

```rust
// Compile-time selection via features
#[cfg(feature = "simulator")]
pub type DefaultP2P = simulator::SimulatorP2P;

#[cfg(feature = "iroh")]
pub type DefaultP2P = iroh::IrohP2P;

// When using Iroh: NodeId = iroh::NodeId (no wrapper)
// When using simulator: NodeId = SimulatorNodeId
// No runtime overhead, direct type usage
```

## Implementation Details

### Web-Server-Like Integration API

**Purpose**: Provide familiar REST-like routing abstraction for MoosicBox integration

**Design**: Mix of HTTP semantics with service modularity
```rust
// Service registration
trait P2PService {
    fn register_routes(&self, router: &mut P2PRouter);
}

// HTTP-like routing
impl P2PRouter {
    pub fn route<H>(&mut self, method: Method, path: &str, handler: H)
    where H: Fn(P2PRequest) -> P2PResponse + Send + Sync + 'static;
}

// Usage example
struct AudioService;
impl P2PService for AudioService {
    fn register_routes(&self, router: &mut P2PRouter) {
        router.route(Method::GET, "/audio/stream/:id", handle_audio_stream);
        router.route(Method::POST, "/audio/metadata", handle_metadata);
    }
}
```

### Simulator Implementation

**Purpose**: Deterministic testing with controllable network conditions

**Architecture**:
- Graph-based network topology simulation
- Controllable time via `switchy_time`
- In-memory message routing with realistic network effects
- Environment variable configuration following switchy patterns

**Network Graph Model**:
```rust
struct NetworkGraph {
    nodes: BTreeMap<SimulatorNodeId, NodeInfo>,
    links: BTreeMap<(SimulatorNodeId, SimulatorNodeId), LinkInfo>,
}

struct LinkInfo {
    latency: Duration,
    packet_loss: f64,
    bandwidth_limit: Option<u64>,
    is_active: bool,
}
```

**Environment Configuration**:
```bash
# Time control (via switchy_time)
SIMULATOR_TIME_MULTIPLIER=1000    # 1000x speed for testing
SIMULATOR_STEP_SIZE_MS=10         # 10ms time steps

# Network conditions
SIMULATOR_DEFAULT_LATENCY_MS=50   # Base latency
SIMULATOR_DEFAULT_PACKET_LOSS=0.01  # 1% packet loss
SIMULATOR_PARTITION_PROBABILITY=0.001  # Network partition chance

# Discovery simulation
SIMULATOR_DISCOVERY_DELAY_MS=100  # DNS lookup delay
SIMULATOR_DNS_TTL_SECONDS=300     # DNS cache TTL
```

**Message Routing**:
```
Alice wants to send to Bob:
1. Alice calls connection.send(data)
2. Simulator looks up network path: Alice -> Router -> Bob
3. Applies latency via switchy_time.sleep(calculated_latency)
4. Applies packet loss via deterministic random
5. Delivers to Bob's message queue
6. Bob receives via connection.recv()
```

**Mock Discovery Service**:
```rust
// Registration
simulator.register_peer("alice", alice_addr);
simulator.register_peer("bob", bob_addr);

// Discovery
let bob_addr = simulator.discover("bob").await?; // Returns registered address
let connection = simulator.connect(bob_addr).await?;
```

### Iroh Implementation

**Purpose**: Production P2P networking with automatic NAT traversal

**Architecture**:
- Direct wrapper around Iroh's `Endpoint` and `Connection` types
- Zero-cost abstraction - no wrapper overhead
- Automatic NAT traversal and relay fallback
- Real cryptographic node identity

**Identity Management**:
```rust
// Direct use of Iroh types for zero cost
type IrohNodeId = iroh::NodeId;  // = iroh::PublicKey

impl P2PNodeId for IrohNodeId {
    fn from_bytes(bytes: &[u8; 32]) -> Result<Self, P2PError> {
        iroh::PublicKey::from_bytes(bytes).map_err(Into::into)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        self.as_bytes()  // Direct delegation
    }

    fn fmt_short(&self) -> String {
        self.fmt_short()  // Direct delegation
    }
}
```

**Connection Handling**:
```rust
struct IrohConnection {
    connection: iroh::Connection,
    // Additional state if needed
}

impl P2PConnection for IrohConnection {
    type NodeId = IrohNodeId;

    async fn send(&mut self, data: &[u8]) -> Result<(), P2PError> {
        let mut stream = self.connection.open_uni().await?;
        stream.write_all(data).await?;
        stream.finish()?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, P2PError> {
        let mut stream = self.connection.accept_uni().await?;
        let data = stream.read_to_end(usize::MAX).await?;
        Ok(data)
    }
}
```

**NAT Traversal Configuration**:
```rust
// Iroh automatically handles:
// - STUN for discovering external IP/port
// - ICE for hole punching
// - Relay fallback when direct connection fails
// - Connection persistence and reconnection

let endpoint = Endpoint::builder()
    .discovery(Box::new(DnsDiscovery::n0_dns()))  // Optional discovery
    .relay_mode(RelayMode::Default)               // Use default relays
    .bind().await?;
## Testing Framework

### Property-Based Testing with Cross-Implementation Compatibility

**Purpose**: Ensure all implementations behave identically and satisfy protocol invariants

**Architecture**:
```rust
// Generic tests that work with any P2PSystem implementation
fn test_basic_communication<S: P2PSystem>(system: S) {
    proptest!(|(message in any_message())| {
        // Test invariant: any message sent is received intact
        let alice = system.create_node();
        let bob = system.create_node();

        let connection = alice.connect(bob.node_id()).await?;
        connection.send(&message).await?;
        let received = bob.accept().await?.recv().await?;

        assert_eq!(message, received);
    });
}

// Cross-implementation compatibility tests
#[test]
fn simulator_and_iroh_compatibility() {
    let simulator_alice = create_simulator_node();
    let iroh_bob = create_iroh_node();

    // Test that simulator can talk to Iroh and vice versa
    // (when running integration tests)
}
```

**Test Categories**:
- **Unit Tests**: Individual component behavior
- **Property Tests**: Protocol invariants and edge cases
- **Integration Tests**: End-to-end communication scenarios
- **Performance Tests**: Latency, throughput, resource usage
- **Cross-Implementation**: Simulator ↔ Iroh compatibility

### Deterministic Testing via Simulator

**Controlled Environment**:
```rust
// Deterministic test setup
let simulator = SimulatorP2P::new()
    .with_seed(12345)                    // Reproducible randomness
    .with_time_control()                 // Manual time advancement
    .with_network_topology(topology);    // Predefined network graph

// Test specific scenarios
simulator.add_network_partition("alice", "bob");
simulator.advance_time(Duration::from_secs(30));
assert!(connection_times_out);

simulator.heal_partition();
simulator.advance_time(Duration::from_secs(5));
assert!(connection_recovers);
```

## Connection Lifecycle Management

### Hybrid Approach (Matching Iroh)

```rust
// Automatic cleanup on drop (RAII)
{
    let connection = p2p.connect(node_id).await?;
    // Connection automatically closed when dropped
} // <- Connection closed here

// Explicit close for immediate closure
let mut connection = p2p.connect(node_id).await?;
connection.close()?; // Immediate closure

// Clone-able handles
let connection1 = p2p.connect(node_id).await?;
let connection2 = connection1.clone(); // Same underlying connection
// Closes when ALL handles are dropped

// Configurable idle timeout
let p2p = P2PBuilder::new()
    .idle_timeout(Duration::from_secs(300))
    .build();
```

### Error Handling - Unified Approach

```rust
#[derive(Debug, thiserror::Error)]
pub enum P2PError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Invalid node ID: {0}")]
    InvalidNodeId(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}
```

## Security and Authentication

### Identity Management
- **Ed25519 Keys**: Cryptographically secure node identity
- **QUIC Encryption**: Built-in transport encryption via Iroh
- **No Additional Auth**: Application decides connection acceptance
- **Key Persistence**: Deterministic keys for testing, secure generation for production

### Network Security
- **Mandatory Encryption**: All communication encrypted via QUIC
- **Peer Authentication**: Public key verification during handshake
- **DoS Protection**: Connection limits and rate limiting at application layer
- **No Plaintext**: No fallback to unencrypted communication

## Resource Management

### Connection Configuration
```rust
// Basic configuration without performance targets
P2PBuilder::new()
    .max_connections(100)
    .connection_timeout(Duration::from_secs(30))
    .max_message_size(1024 * 1024) // 1MB
    .build()
```

### Zero-Cost Abstractions
- When using Iroh: Direct type usage, no wrapper overhead
- When using simulator: Minimal abstraction for testing
- Compile-time backend selection via features

## MoosicBox Integration Strategy

### Web-Server-Like API for Familiar Integration

```rust
// Initialize P2P system
let p2p_system = P2PBuilder::new()
    .backend(DefaultP2P::new())  // Compile-time selected
    .build();

// Create router with HTTP-like semantics
let mut router = P2PRouter::new();

// Register services (similar to web route registration)
let audio_service = AudioService::new();
audio_service.register_routes(&mut router);

let sync_service = SyncService::new();
sync_service.register_routes(&mut router);

// Start P2P server
p2p_system.serve(router).await?;

// Service implementation example
impl P2PService for AudioService {
    fn register_routes(&self, router: &mut P2PRouter) {
        router.route(Method::GET, "/audio/stream/:id", |req| {
            let track_id = req.path_param("id")?;
            let audio_data = self.get_audio_stream(track_id)?;
            P2PResponse::ok(audio_data)
        });

        router.route(Method::POST, "/audio/metadata", |req| {
            let metadata: AudioMetadata = deserialize(&req.body)?;
            self.update_metadata(metadata)?;
            P2PResponse::ok(b"updated")
        });
    }
}
```

### Compile-Time Backend Selection

```toml
# Cargo.toml for testing
[dependencies.moosicbox_p2p]
features = ["simulator"]

# Cargo.toml for production
[dependencies.moosicbox_p2p]
features = ["iroh"]
```

```rust
// Automatic selection based on features
use moosicbox_p2p::DefaultP2P;

// In simulator build: DefaultP2P = SimulatorP2P
// In iroh build: DefaultP2P = IrohP2P
let p2p = DefaultP2P::new(config);
```

### Migration from Tunnel Server

**Phase 1**: P2P as Alternative (No Tunnel Fallback)
```rust
// Clean separation - choose one at compile time
match env::var("MOOSICBOX_TRANSPORT") {
    Ok("p2p") => {
        let transport = P2PTransport::new();
        server.use_transport(transport);
    }
    _ => {
        let transport = TunnelTransport::new();
        server.use_transport(transport);
    }
}
```

**Phase 2**: Service-by-Service Migration
- Audio streaming → P2P first
- Metadata sync → P2P second
- Control messages → P2P last
- Independent rollout per service

**Phase 3**: Tunnel Deprecation
- Remove tunnel dependencies
- P2P becomes default
- Clean up migration code

## Configuration and Environment Integration

### Switchy Pattern Integration

```bash
# Time control (development/testing)
SIMULATOR_TIME_MULTIPLIER=1000
SIMULATOR_STEP_SIZE_MS=10

# Network simulation
SIMULATOR_DEFAULT_LATENCY_MS=50
SIMULATOR_PACKET_LOSS=0.01

# Production configuration
P2P_LISTEN_PORT=8800
P2P_RELAY_MODE=default
P2P_DISCOVERY_MODE=mdns
```

### Development vs Production

```rust
// Development: Fast simulation
#[cfg(debug_assertions)]
let p2p = P2PBuilder::new()
    .backend(SimulatorP2P::new())
    .fast_mode()  // Minimal delays
    .build();

// Production: Real networking
#[cfg(not(debug_assertions))]
let p2p = P2PBuilder::new()
    .backend(IrohP2P::new())
    .discovery_mode(DiscoveryMode::MdnsAndDht)
    .relay_mode(RelayMode::Default)
    .build();
```

## Monitoring and Observability

### Integration with Existing Telemetry

```rust
// Use existing MoosicBox telemetry patterns
use moosicbox_telemetry::{track_latency, track_counter};

impl P2PConnection for IrohConnection {
    async fn send(&mut self, data: &[u8]) -> Result<(), P2PError> {
        let _timer = track_latency("p2p.message.send");
        track_counter("p2p.messages.sent", 1);

        // Actual send implementation
        let result = self.inner_send(data).await;

        if result.is_err() {
            track_counter("p2p.errors.send", 1);
        }

        result
    }
}
```

### Health Checks and Diagnostics

```rust
// Health check endpoint
router.route(Method::GET, "/health", |_req| {
    let health = P2PHealth {
        active_connections: p2p.connection_count(),
        node_id: p2p.local_node_id().fmt_short(),
        network_type: if p2p.has_direct_connections() { "direct" } else { "relay" },
        uptime: p2p.uptime(),
    };
    P2PResponse::json(health)
});
```

## Implementation Validation

### Success Criteria Summary

**Functional Requirements**:
- [x] Zero-cost abstraction when using Iroh
- [x] Deterministic testing via simulator
- [x] Web-server-like integration API
- [x] Raw bytes transport (no serialization lock-in)
- [x] Ed25519 node identity matching Iroh

**Technical Requirements**:
- [ ] Zero-cost abstraction when using Iroh backend
- [ ] No wrapper overhead in production builds
- [ ] Deterministic behavior in simulator mode
- [ ] Resource cleanup prevents leaks

**Quality Requirements**:
- [ ] Zero clippy warnings with fail-on-warnings
- [ ] Property tests pass on all implementations
- [ ] Cross-implementation compatibility tests pass
- [ ] Documentation covers all public APIs
- [ ] Examples demonstrate real-world usage

## Implementation Guidelines

All design decisions are finalized and documented in [`clarifications.md`](./clarifications.md). This section provides concrete implementation guidance to eliminate ambiguity.

### Dependency Philosophy
**Just-in-time dependencies**: Add only when immediately used
- **Phase 1**: Zero dependencies (completely empty package)
- **Phase 2.1**: First dependencies: `switchy_async`, `switchy_time`, `switchy_random`
- **Phase 4.1**: Add `thiserror` when creating error types
- **Phase 7.1**: Add `iroh` when implementing real P2P
- **Phase 8.1**: Add `proptest` when implementing property tests

**Verify with tooling**: Run `cargo machete` after each phase
**Document reasoning**: Explain why each dependency is needed in commit messages

### Code Organization Principles
**Start simple**: All code in `lib.rs` initially
- **Phase 1**: Only clippy configuration in `lib.rs`
- **Phase 2**: Add `mod simulator;` when implementing simulator
- **Phase 3**: Add `mod traits;` when extracting traits
- **Phase 4**: Add `mod types;` when adding error types

**Extract gradually**: Move to modules when files exceed 500 lines
**Feature-based**: Group by feature (`simulator.rs`) not type (`traits.rs`)

### Testing Philosophy
**Test-driven**: Write tests before implementation
- Four critical test scenarios drive simulator implementation
- All tests must pass for phase completion
- Property-based tests for edge cases and invariants

**Property-based**: Use proptest for invariants
```rust
proptest! {
    #[test]
    fn message_integrity(data in any::<Vec<u8>>()) {
        // Property: Any data sent is received unchanged
        let received = send_and_receive(data.clone());
        assert_eq!(data, received);
    }
}
```

**Cross-implementation**: Same tests for simulator and Iroh
- Generic test functions work with any `P2PSystem`
- Ensures simulator and Iroh behave identically
- Prevents implementation-specific behavior

### Error Handling Standards
**Single error type**: Flat `P2PError` enum (no nested errors)
```rust
#[derive(Debug, Error)]
pub enum P2PError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    // All other variants...
}
```

**Explicit variants**: Named variants for each error case
**No panics**: All errors returned as `P2PResult<T>`, never panic in library code

### Performance Goals
**Zero-cost abstractions**: Associated types, not trait objects
- Use `trait P2PSystem { type NodeId: P2PNodeId; }` not `Box<dyn P2PNodeId>`
- Direct type aliases: `type IrohNodeId = iroh::NodeId` (no wrapper)

**Predictable performance targets**:
- Connection establishment: < 100ms local, < 500ms remote
- Message latency: < 10ms local, < 100ms remote
- Memory efficiency: < 1MB per connection
- Scalability: Support 1000+ concurrent connections

### Feature Flag Design
**Mutually exclusive backends**:
```toml
[features]
default = ["simulator"]
simulator = ["dep:switchy_async", "dep:switchy_time", "dep:switchy_random"]
iroh = ["dep:iroh"]
test-utils = ["dep:proptest"]
```

**Compile-time selection**:
```rust
#[cfg(feature = "simulator")]
pub type DefaultP2P = simulator::SimulatorP2P;

#[cfg(feature = "iroh")]
pub type DefaultP2P = iroh::IrohP2P;
```

### Documentation Standards
**Every public item** needs rustdoc with working example:
```rust
/// Connect to a remote peer by node ID
///
/// # Examples
///
/// ```
/// use moosicbox_p2p::*;
/// let p2p = SimulatorP2P::new();
/// let connection = p2p.connect(node_id).await?;
/// ```
///
/// # Errors
///
/// Returns `P2PError::NoRoute` if no path exists to the peer.
/// Returns `P2PError::Timeout` if connection takes longer than 30 seconds.
///
/// # Performance
///
/// Connection establishment typically completes in < 100ms for local peers.
pub async fn connect(&self, node_id: NodeId) -> P2PResult<Connection> { ... }
```

**Include examples** for non-obvious APIs
**Explain "why"** not just "what" - design rationale and trade-offs
**Link to relevant** specifications or external documentation

### Migration Strategy from Tunnel
**Clean separation**: P2P is standalone alternative, not fallback
- No tunnel dependencies in P2P code
- Feature flags control which transport system is used
- Same high-level API where possible (compatibility layer)

**Service-by-service migration**:
1. Audio streaming → P2P first (high bandwidth benefit)
2. Metadata sync → P2P second (reduced latency benefit)
3. Control messages → P2P last (minimal benefit, but consistency)

**Rollback plan**: Keep tunnel code until P2P proven stable
- Feature flags allow instant rollback without code changes
- Monitor both systems during transition period
- Document rollback procedures and triggers

### Security Considerations
**Identity management**: Peers authenticated via ed25519 public key cryptography
- Application layer decides authorization policies
- No built-in user management or permissions

**Transport security**: All connections encrypted via QUIC/TLS 1.3
- No unencrypted data transmission ever
- Perfect forward secrecy where possible
- No downgrade attacks possible

**DoS protection**: Rate limiting and resource management at application layer
- Connection rate limiting per peer
- Message rate limiting per connection
- Resource quotas for memory and bandwidth

### Next Steps

This architecture document, combined with the clarifications and detailed plan, provides a complete and unambiguous technical foundation for implementation.

**Phase 1 is ready to begin** following the explicit instructions in [`plan.md`](./plan.md).
