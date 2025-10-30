#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::too_many_lines)]

//! Basic P2P Connection Example
//!
//! This example demonstrates the fundamental operations of the `switchy_p2p` library:
//! - Creating P2P nodes with deterministic IDs
//! - Understanding node identity and short ID formatting
//! - Peer discovery with name registration
//! - Basic P2P system API usage

use switchy_p2p::simulator::SimulatorP2P;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Switchy P2P Basic Connection Example ===\n");

    // Step 1: Create P2P nodes with deterministic seed values
    // Using seeds ensures consistent node IDs for testing and demonstration
    println!("Step 1: Creating P2P nodes with deterministic IDs...");
    let alice = SimulatorP2P::with_seed("alice");
    let bob_node = SimulatorP2P::with_seed("bob");
    let carol_node = SimulatorP2P::with_seed("carol");

    let alice_id = alice.local_node_id();
    let bob_id = bob_node.local_node_id();
    let carol_id = carol_node.local_node_id();

    println!("  Alice's Node ID: {}", alice_id.fmt_short());
    println!("  Bob's Node ID:   {}", bob_id.fmt_short());
    println!("  Carol's Node ID: {}", carol_id.fmt_short());
    println!();

    // Step 2: Demonstrate node ID properties
    println!("Step 2: Node ID properties...");
    println!("  Full Alice ID: {alice_id}");
    println!(
        "  Short format (first 10 hex chars): {}",
        alice_id.fmt_short()
    );
    println!("  Node IDs are 256-bit (32 bytes) values");
    println!(
        "  Same seed always produces same ID: {}",
        SimulatorP2P::with_seed("alice").local_node_id() == alice_id
    );
    println!();

    // Step 3: Random node ID generation
    println!("Step 3: Random node ID generation...");
    let random_node1 = SimulatorP2P::new();
    let random_node2 = SimulatorP2P::new();
    println!(
        "  Random Node 1 ID: {}",
        random_node1.local_node_id().fmt_short()
    );
    println!(
        "  Random Node 2 ID: {}",
        random_node2.local_node_id().fmt_short()
    );
    println!(
        "  Random IDs are different: {}",
        random_node1.local_node_id() != random_node2.local_node_id()
    );
    println!();

    // Step 4: Register peers in discovery system
    // Each node maintains its own discovery registry in its network graph
    println!("Step 4: Registering peers for discovery...");
    alice
        .register_peer("alice-service", alice_id.clone())
        .await
        .map_err(|e| format!("Failed to register Alice: {e}"))?;
    alice
        .register_peer("bob-service", bob_id.clone())
        .await
        .map_err(|e| format!("Failed to register Bob: {e}"))?;
    alice
        .register_peer("carol-service", carol_id.clone())
        .await
        .map_err(|e| format!("Failed to register Carol: {e}"))?;
    println!("  Registered alice-service, bob-service, and carol-service");
    println!("  Names can be used to discover node IDs without knowing them upfront");
    println!();

    // Step 5: Discover peers by name
    // Discovery provides DNS-like name resolution for node IDs
    println!("Step 5: Discovering peers by name...");
    let discovered_bob = alice
        .discover("bob-service")
        .await
        .map_err(|e| format!("Failed to discover Bob: {e}"))?;
    println!("  Discovered bob-service: {}", discovered_bob.fmt_short());
    println!("  IDs match: {}", discovered_bob == *bob_id);

    let discovered_carol = alice
        .discover("carol-service")
        .await
        .map_err(|e| format!("Failed to discover Carol: {e}"))?;
    println!(
        "  Discovered carol-service: {}",
        discovered_carol.fmt_short()
    );
    println!("  IDs match: {}", discovered_carol == *carol_id);
    println!();

    // Step 6: Test discovery failure
    println!("Step 6: Testing discovery of non-existent peer...");
    match alice.discover("non-existent-service").await {
        Ok(id) => println!("  Unexpectedly found: {}", id.fmt_short()),
        Err(e) => println!("  Expected error: {e}"),
    }
    println!();

    // Step 7: Demonstrate ID comparison
    println!("Step 7: Node ID comparison...");
    println!("  Alice == Bob: {}", alice_id == bob_id);
    let alice_id_clone = alice_id.clone();
    println!("  Alice == Alice (clone): {}", alice_id == &alice_id_clone);
    println!("  Bob == Carol: {}", bob_id == carol_id);
    println!();

    println!("=== Example completed successfully! ===");
    println!();
    println!("This example demonstrated:");
    println!("  - Deterministic node ID generation with seeds");
    println!("  - Random node ID generation");
    println!("  - Node ID formatting (full and short)");
    println!("  - Peer registration in discovery system");
    println!("  - DNS-like peer discovery by name");
    println!("  - Node ID comparison and equality");
    println!();
    println!("The switchy_p2p library provides abstractions for P2P networking");
    println!("that can be implemented over various transport layers (simulator,");
    println!("Iroh, etc.). This example used the built-in network simulator.");
    Ok(())
}
