# Basic P2P Connection Example

A comprehensive example demonstrating the fundamental operations of the `switchy_p2p` library, including node ID generation, peer discovery, and basic P2P system API usage.

## What This Example Demonstrates

- Creating P2P nodes with deterministic seed values for testing
- Generating random node IDs for production use
- Node identification with 256-bit IDs and short display format
- Registering peers in the discovery system (DNS-like naming)
- Discovering peers by name instead of node ID
- Node ID comparison and equality checks
- Error handling with discovery failures
- Basic P2P system abstraction patterns

## Prerequisites

- Basic understanding of async/await in Rust
- Familiarity with the tokio runtime
- Understanding of P2P networking concepts (nodes, peer discovery)

## Running the Example

```bash
cargo run --manifest-path packages/p2p/examples/basic_connection/Cargo.toml
```

Or from the example directory:

```bash
cd packages/p2p/examples/basic_connection
cargo run
```

## Expected Output

```
=== Switchy P2P Basic Connection Example ===

Step 1: Creating P2P nodes with deterministic IDs...
  Alice's Node ID: 784cc22bf4
  Bob's Node ID:   eac14ba6d8
  Carol's Node ID: 3a593e488f

Step 2: Node ID properties...
  Full Alice ID: 784cc22bf481efb3e0d8c1cc8579ee164ecfda632d8459a596a890cbaf365d6f
  Short format (first 10 hex chars): 784cc22bf4
  Node IDs are 256-bit (32 bytes) values
  Same seed always produces same ID: true

Step 3: Random node ID generation...
  Random Node 1 ID: b810d99d24
  Random Node 2 ID: f418de02f3
  Random IDs are different: true

Step 4: Registering peers for discovery...
  Registered alice-service, bob-service, and carol-service
  Names can be used to discover node IDs without knowing them upfront

Step 5: Discovering peers by name...
  Discovered bob-service: eac14ba6d8
  IDs match: true
  Discovered carol-service: 3a593e488f
  IDs match: true

Step 6: Testing discovery of non-existent peer...
  Expected error: Name 'non-existent-service' not found

Step 7: Node ID comparison...
  Alice == Bob: false
  Alice == Alice (clone): true
  Bob == Carol: false

=== Example completed successfully! ===

This example demonstrated:
  - Deterministic node ID generation with seeds
  - Random node ID generation
  - Node ID formatting (full and short)
  - Peer registration in discovery system
  - DNS-like peer discovery by name
  - Node ID comparison and equality

The switchy_p2p library provides abstractions for P2P networking
that can be implemented over various transport layers (simulator,
Iroh, etc.). This example used the built-in network simulator.
```

## Code Walkthrough

### Creating P2P Nodes with Deterministic IDs

The example starts by creating P2P nodes with deterministic IDs using seed values:

```rust
let alice = SimulatorP2P::with_seed("alice");
let bob_node = SimulatorP2P::with_seed("bob");
let carol_node = SimulatorP2P::with_seed("carol");
```

Using seeds ensures consistent node IDs across runs, which is useful for testing and debugging. Each node gets a unique 256-bit identifier derived from the seed using a hash function.

### Node ID Properties

Every node has a 256-bit (32-byte) identifier that can be displayed in multiple formats:

```rust
let alice_id = alice.local_node_id();
println!("Full ID: {}", alice_id);              // Full 64-char hex string
println!("Short ID: {}", alice_id.fmt_short()); // First 10 chars
```

The `fmt_short()` method provides a human-readable abbreviated format, which is useful for logging and display purposes.

### Random Node ID Generation

For production use, nodes can generate random IDs:

```rust
let random_node = SimulatorP2P::new();
```

Each call to `new()` generates a cryptographically random 256-bit identifier, ensuring uniqueness across the network.

### Peer Discovery System

The discovery system allows nodes to register human-readable names:

```rust
alice.register_peer("bob-service", bob_id.clone()).await?;
```

Other peers can then discover nodes by name without knowing their node IDs:

```rust
let discovered_id = alice.discover("bob-service").await?;
```

This acts like a DNS system for P2P networks, making it easier to establish connections without requiring out-of-band communication of node IDs.

### Discovery Error Handling

The example demonstrates proper error handling for failed discovery attempts:

```rust
match alice.discover("non-existent-service").await {
    Ok(id) => println!("Found: {}", id.fmt_short()),
    Err(e) => println!("Error: {}", e),
}
```

Discovery failures return descriptive errors, allowing applications to handle missing peers gracefully.

### Node ID Comparison

Node IDs support standard comparison and equality operations:

```rust
if alice_id == bob_id {
    println!("Same node!");
} else {
    println!("Different nodes");
}
```

IDs can be cloned and compared, which is useful for routing decisions and connection management.

## Key Concepts

### Node Identity

Each P2P node has a unique 256-bit identifier that:

- Remains constant for the lifetime of the node
- Can be deterministically generated from seeds (for testing)
- Can be randomly generated (for production)
- Supports both full and abbreviated display formats
- Is comparable and cloneable

### Discovery vs Direct Connection

- **Discovery** (`register_peer`/`discover`) - Name-based peer lookup, useful when node IDs aren't known upfront
- **Direct connection** (future feature) - Direct connection when you already have the node ID

This example focuses on the discovery aspects of the P2P system, demonstrating how nodes can be located by name.

### P2P System Abstraction

The `P2PSystem` trait provides a generic interface that can be implemented over different transport layers:

- **Simulator** - Built-in network simulator for testing and development
- **Iroh** (planned) - Production P2P networking with NAT traversal
- **Custom implementations** - You can implement the trait for your own transport

### Async Operations

All discovery operations are async and must be awaited:

- `register_peer()` - Registers a name in the discovery system
- `discover()` - Looks up a peer by name (includes simulated network delay)

The simulator adds realistic delays to discovery operations to model real-world network conditions.

## Testing the Example

### Modifying Seeds

You can change the seed values to generate different deterministic node IDs:

```rust
let alice = SimulatorP2P::with_seed("my-custom-seed");
```

The same seed will always produce the same node ID, allowing for reproducible testing.

### Adjusting Discovery Delay

The simulator's discovery delay can be configured via environment variable:

```bash
SIMULATOR_DISCOVERY_DELAY_MS=200 cargo run --manifest-path packages/p2p/examples/basic_connection/Cargo.toml
```

This increases the simulated DNS lookup time to 200ms (default is 100ms).

### Adding More Peers

Extend the example by creating and registering additional nodes:

```rust
let dave = SimulatorP2P::with_seed("dave");
alice.register_peer("dave-service", dave.local_node_id().clone()).await?;
let discovered_dave = alice.discover("dave-service").await?;
```

### Testing Random IDs

Uncomment or add more random node generation to see unique IDs:

```rust
for i in 0..5 {
    let node = SimulatorP2P::new();
    println!("Random node {}: {}", i, node.local_node_id().fmt_short());
}
```

## Troubleshooting

### "Name not found" Error

**Cause:** The peer name wasn't registered before attempting discovery.

**Solution:** Ensure `register_peer()` is called and awaited before `discover()`:

```rust
alice.register_peer("service-name", node_id.clone()).await?;
// Now discovery will work
let discovered = alice.discover("service-name").await?;
```

### Different Node IDs on Each Run (with seeds)

**Cause:** The seed value might be changing between runs, or you're not using seeds.

**Solution:** Use `with_seed()` instead of `new()` for deterministic IDs:

```rust
let node = SimulatorP2P::with_seed("consistent-seed"); // Always same ID
```

### Same Node IDs for Different Seeds

**Cause:** This shouldn't happen - if it does, there may be a hash collision or implementation issue.

**Solution:** Use different seed values that are sufficiently different (avoid similar strings).

## Related Examples

This is currently the only example for `switchy_p2p`. Future examples may include:

- Full connection establishment and message passing (requires shared network graph)
- Network partition and healing demonstration
- Multi-hop routing scenarios
- Performance testing with many nodes
