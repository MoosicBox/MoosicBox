# P2P Implementation Clarifications

This document captures all the implementation decisions made during the specification phase to eliminate ambiguities and provide clear direction for implementation.

## Design Decisions Summary

### 1. Async Runtime: `switchy_async`
**Decision**: Use `switchy_async` throughout the P2P system
- Provides abstraction over tokio vs simulator runtime
- Consistent with MoosicBox patterns
- Enables deterministic testing via simulator runtime
- All trait methods use `switchy_async` types

### 2. Time Management: `switchy_time`
**Decision**: Use `switchy_time` for all time operations
- Enables deterministic time control in simulator
- Real-time pass-through in production
- Supports testing timeouts, delays, and retry logic
- Integration with existing switchy ecosystem

### 3. Node Identity Strategy
**Decision**: Match Iroh's approach exactly
- `NodeId = PublicKey` (32-byte ed25519 public key)
- Use real cryptographic keys in both simulator and production
- Z-base-32 encoding for display/parsing
- `fmt_short()` for abbreviated display (first 5 bytes)
- Provide test helpers for deterministic key generation from seeds
- Zero-cost abstraction via trait with associated types

```rust
// Test helper example
let alice_key = test_node_id("alice"); // deterministic from seed
let bob_key = test_node_id("bob");
```

### 4. Network Topology Simulation
**Decision**: Graph-based routing model
- Explicit network topology as a graph
- Support for complex scenarios:
  - NAT traversal and hole punching
  - Relay server routing
  - Network partitions and healing
  - Asymmetric connections
  - Multi-hop scenarios
- Most realistic simulation for P2P testing

### 5. Message Serialization
**Decision**: Raw bytes only at P2P layer
- P2P traits work with `&[u8]` and `Vec<u8>`
- Application layer handles serialization (JSON, bincode, protobuf, etc.)
- Matches Iroh's approach exactly
- Clean separation of concerns
- Maximum flexibility for different message types

### 6. Discovery Service Integration
**Decision**: Mock DNS-like discovery in simulator
- Simulator provides built-in name resolution
- `simulator.register("alice", alice_addr)`
- `discover("alice")` returns registered address
- Tests discovery patterns without implementing complex protocols
- Production uses real discovery (mDNS, DHT, DNS)

### 7. Error Handling Strategy
**Decision**: Single `P2PError` enum
- One unified error type for all P2P operations
- Variants added as needed during implementation
- Uses `thiserror` for clean error derivation
- Simplifies error handling throughout codebase
- Aligns with fail-on-warnings requirement

### 8. Transport Priorities
**Decision**: Streams first, datagrams later
- Initial implementation focuses on reliable, ordered streams
- Covers most MoosicBox use cases (metadata, control, file transfer)
- Datagrams added in later phase for real-time audio
- Incremental complexity approach

### 9. Testing Strategy
**Decision**: Multi-layered approach
- **Property-based testing**: Use proptest for invariants and edge cases
- **Unit tests**: Test individual components in isolation
- **Integration tests**: Full scenario tests using simulator
- **Example scenarios**: Common use case demonstrations
- Focus on finding bugs through property testing

### 10. Package Structure
**Decision**: Single crate with feature flags
- `moosicbox_p2p` package with optional features
- Features: `simulator` (default), `iroh`
- Dependencies only pulled for enabled features
- Follows MoosicBox package conventions
- Simpler than workspace approach

**Dependency Management**:
- All dependencies use `{ workspace = true }` pattern
- Never specify version numbers in package `Cargo.toml`
- New dependencies added to workspace with latest full semantic version
- Required workspace additions: `iroh = "0.91.2"`, `proptest = "1.7.0"`

### 11. Runtime Selection
**Decision**: Compile-time only via features
- `cargo build --features simulator` for testing
- `cargo build --features iroh` for production
- Zero runtime overhead - no dynamic dispatch
- Clean separation, no fallback complexity

**Async Runtime Abstraction**:
- Use `switchy_async` for all async operations inside the library
- NO direct `tokio::` imports in the P2P library code
- Iroh brings tokio transitively - we don't add it as direct dependency
- Examples may use `#[tokio::main]` for simplicity, but production code uses switchy

### 12. Connection Lifecycle Management
**Decision**: Hybrid approach matching Iroh
- **RAII**: Connections auto-close when all handles dropped
- **Explicit close**: `close()` method for immediate closure
- **Idle timeout**: Configurable connection timeouts
- **Clone-able**: Multiple handles to same connection
- Matches Iroh's Connection behavior exactly

### 13. Zero-Cost Abstractions
**Decision**: Trait with associated types
```rust
trait P2PSystem {
    type NodeId: P2PNodeId;
    type Connection: P2PConnection;
    // No wrapper types, direct use of implementation types
}

// In production: type NodeId = iroh::NodeId (zero cost)
// In simulator: type NodeId = SimulatorNodeId
```

### 14. Integration API Design
**Decision**: Web-server-like abstraction
- REST-like routing: `p2p.route(Method::GET, "/tracks/:id", handler)`
- Service modularity: Services register their own routes
- P2P layer handles routing like a web framework
- Familiar abstraction similar to tunnel server
- Mix of HTTP semantics with service encapsulation

## Implementation Approach

### Development Philosophy
1. **Build concrete first, extract abstractions second**
   - Start with working simulator implementation
   - Extract traits from working code
   - Avoids unused code warnings and over-abstraction

2. **Every piece of code must be immediately used**
   - No speculative generalization
   - Features driven by actual needs
   - Test-driven development approach

3. **Compile-time polymorphism**
   - Feature flags for backend selection
   - Associated types for zero-cost abstraction
   - No runtime dispatch overhead

### Package Organization
```
packages/p2p/
├── Cargo.toml          # Features: simulator, iroh
├── src/
│   ├── lib.rs          # Traits and public API
│   ├── types.rs        # Common types and errors
│   ├── simulator.rs    # Simulator implementation
│   ├── iroh.rs        # Iroh implementation
│   └── test_utils.rs   # Testing utilities
├── tests/              # Integration tests
├── examples/           # Usage examples
└── benches/           # Performance benchmarks
```

### Feature Configuration
```toml
[features]
default = ["simulator"]
simulator = ["dep:switchy_async", "dep:switchy_time", ...]
iroh = ["dep:iroh", "dep:iroh-net", ...]
fail-on-warnings = []
```

## Core Architecture

### Trait Hierarchy
```rust
// Core system trait
trait P2PSystem: Send + Sync + 'static {
    type NodeId: P2PNodeId;
    type Connection: P2PConnection;
    type Listener: P2PListener;

    async fn connect(&self, node_id: Self::NodeId) -> Result<Self::Connection, P2PError>;
    async fn listen(&self, addr: &str) -> Result<Self::Listener, P2PError>;
    fn local_node_id(&self) -> &Self::NodeId;
}

// Node identity trait
trait P2PNodeId: Clone + Debug + Display + Send + Sync + 'static {
    fn from_bytes(bytes: &[u8; 32]) -> Result<Self, P2PError>;
    fn as_bytes(&self) -> &[u8; 32];
    fn fmt_short(&self) -> String;
}

// Connection trait (streams only initially)
trait P2PConnection: Send + Sync + 'static {
    async fn send(&mut self, data: &[u8]) -> Result<(), P2PError>;
    async fn recv(&mut self) -> Result<Vec<u8>, P2PError>;
    fn remote_node_id(&self) -> &Self::NodeId;
    fn is_connected(&self) -> bool;
    fn close(&mut self) -> Result<(), P2PError>;
}

// Listener trait
trait P2PListener: Send + Sync + 'static {
    async fn accept(&mut self) -> Result<Self::Connection, P2PError>;
    fn local_addr(&self) -> &str;
}
```

### Web-Server-Like API
```rust
// Service registration
trait P2PService {
    fn register_routes(&self, router: &mut P2PRouter);
}

// Router with HTTP-like semantics
struct P2PRouter {
    // Maps (method, path) -> handler
}

impl P2PRouter {
    fn route<H>(&mut self, method: Method, path: &str, handler: H)
    where H: Fn(P2PRequest) -> P2PResponse + Send + Sync + 'static;
}

// Request/Response types
struct P2PRequest {
    method: Method,
    path: String,
    body: Vec<u8>,
    remote_node_id: NodeId,
}

struct P2PResponse {
    status: StatusCode,
    body: Vec<u8>,
}
```

## Simulator Implementation Details

### Network Graph Model
```rust
struct NetworkGraph {
    nodes: BTreeMap<NodeId, NodeInfo>,
    links: BTreeMap<(NodeId, NodeId), LinkInfo>,
}

struct LinkInfo {
    latency: Duration,
    packet_loss: f64,
    bandwidth_limit: Option<u64>,
    is_active: bool,
}
```

### Message Routing
- Messages traverse explicit network paths
- Latency simulation via `switchy_time`
- Packet loss via deterministic random number generation
- Support for network partitions and asymmetric connections

### Environment Configuration
```bash
# Time control
SIMULATOR_TIME_MULTIPLIER=1000  # 1000x speed
SIMULATOR_STEP_SIZE_MS=10       # 10ms steps

# Network conditions
SIMULATOR_DEFAULT_LATENCY_MS=50
SIMULATOR_DEFAULT_PACKET_LOSS=0.01
SIMULATOR_PARTITION_PROBABILITY=0.001

# Discovery
SIMULATOR_DISCOVERY_DELAY_MS=100
SIMULATOR_DNS_TTL_SECONDS=300
```

## Iroh Integration Details

### Identity Management
- Use Iroh's `SecretKey` and `PublicKey` directly
- `NodeId = iroh::NodeId` (type alias, zero cost)
- Key persistence and generation handled by Iroh

### Connection Handling
- Wrap Iroh's `Connection` and `Endpoint` types
- QUIC streams for reliable message transport
- Automatic NAT traversal via Iroh's networking
- Relay fallback when direct connection fails

### Discovery Integration
- Use Iroh's discovery mechanisms (mDNS, DHT)
- Optional integration with external discovery services
- Support for static peer lists

## Testing Framework

### Property-Based Testing
```rust
proptest! {
    #[test]
    fn any_two_connected_nodes_can_exchange_messages(
        node_a in any_node_id(),
        node_b in any_node_id(),
        message in any_message()
    ) {
        // Test invariant across all implementations
    }
}
```

### Cross-Implementation Tests
- Same test suite runs against simulator and Iroh
- Ensures compatibility and behavior consistency
- Property tests verify protocol invariants

### Performance Benchmarks
- Connection establishment latency
- Message throughput
- Memory usage profiling
- Comparison with tunnel baseline

## Migration Strategy

### Phased Rollout
1. **Phase 1**: P2P available alongside tunnel
2. **Phase 2**: Feature flag controls which system is used
3. **Phase 3**: Gradual migration of services to P2P
4. **Phase 4**: Tunnel deprecation and removal

### Configuration Strategy
```rust
// Environment-based selection
match env::var("MOOSICBOX_P2P_BACKEND") {
    Ok("p2p") => use_p2p_backend(),
    _ => use_tunnel_backend(), // default during migration
}
```

### No Automatic Fallback
- Clean separation between P2P and tunnel systems
- Explicit choice, no runtime switching
- Reduces complexity and potential failure modes

## Success Criteria

### Functional Requirements
- [x] Specification complete and unambiguous
- [ ] Working simulator implementation
- [ ] Iroh implementation with same API
- [ ] Cross-implementation test compatibility
- [ ] Web-server-like integration API

### Technical Requirements
- Zero-cost abstractions (no wrapper overhead in production)
- Deterministic testing via simulator
- Resource cleanup prevents leaks
- Clean separation between backends

### Quality Requirements
- Zero clippy warnings with fail-on-warnings
- Property tests find no invariant violations
- Cross-implementation compatibility
- Documentation covers all use cases
- Examples demonstrate real-world usage

## Next Steps

The specification is now complete and ready for implementation. All ambiguities have been resolved and concrete technical decisions have been made. Implementation should follow the phased approach outlined in plan.md, starting with Phase 1: Package Creation and Setup.
