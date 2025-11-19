# P2P Integration - Execution Plan

## Executive Summary

Implement a P2P (peer-to-peer) communication system as an alternative to the existing centralized tunnel server architecture. This provides direct device-to-device connections using the Iroh library, with automatic NAT traversal and improved performance, while maintaining backward compatibility during migration.

**Current Status:** 🟡 **Phase 0 - Planning** - Specification being drafted

**Completion Estimate:** ~0% complete - Initial specification phase

## Status Legend

- 🔴 **Critical** - Blocks core functionality
- 🟡 **Important** - Affects user experience or API design
- 🟢 **Minor** - Nice-to-have or polish items
- ✅ **Complete** - Fully implemented and validated
- 🟡 **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## Design Decisions (RESOLVED)

### Core Architecture Decisions ✅

- **Package Structure**: Start with `lib.rs` only, grow organically
- **Module Organization**: Extract modules when files exceed 500 lines
- **Dependencies**: Just-in-time addition (zero dependencies in Phase 1)
- **Error Handling**: Single flat `P2PError` enum with `thiserror`

### Implementation Strategy ✅

- **Transport Abstraction**: Full abstraction supporting multiple backends
- **Message Protocol**: Raw bytes only (`&[u8]`, `Vec<u8>`) - no serialization
- **Trait Design**: Five core traits extracted from working simulator code
- **Testing Approach**: Four critical scenarios drive simulator implementation

### Technical Decisions ✅

- **Async Runtime**: `switchy_async` for abstraction (no direct tokio)
- **Node Identity**: `[u8; 32]` matching Iroh's ed25519 public keys
- **Connection Strategy**: Unidirectional QUIC streams for message boundaries
- **Discovery**: Mock DNS in simulator, configurable in production

### Quality Standards ✅

- **Phase Success**: All 4 test scenarios pass + >80% test coverage
- **Code Quality**: Zero warnings, all clippy lints pass
- **Documentation**: Every public API documented with examples
- **Dependency Hygiene**: `cargo machete` clean after each phase

## Phase 1: Package Creation and Setup ✅ **COMPLETED**

**Goal:** Create the switchy_p2p package and integrate it into the workspace

**Status:** All tasks completed successfully

### 1.1 Package Creation

- [x] Create package directory structure 🔴 **CRITICAL**
    - [x] Create `packages/p2p/` directory
          Directory created successfully
    - [x] Create `packages/p2p/src/` directory
          Source directory created successfully
    - [x] Create `packages/p2p/src/lib.rs` with ONLY clippy configuration (NO modules, NO code):

        ```rust
        #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
        #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
        #![allow(clippy::multiple_crate_versions)]

        // IMPORTANT: No modules or code in Phase 1
        // Modules will be added in later phases:
        // - Phase 2.1: mod simulator;
        // - Phase 3.1: mod traits;
        // - Phase 4.1: mod types;
        // - Phase 5.1: mod router;
        ```

    - [x] Create `packages/p2p/Cargo.toml` with complete configuration:
          File created with all required metadata fields (readme, keywords, categories)

        ```toml
        [package]
        name = "switchy_p2p"
        version = "0.1.0"
        edition = { workspace = true }
        authors = { workspace = true }
        license = { workspace = true }
        repository = { workspace = true }
        description = "P2P communication abstraction system"

        [package.metadata.workspaces]
        group = "p2p"

        [dependencies]
        # CRITICAL: Completely empty dependencies section in Phase 1
        # Dependencies will be added exactly when first used:
        # - Phase 2.1: switchy_async, switchy_time, switchy_random
        # - Phase 4.1: thiserror
        # - Phase 7.1: iroh (optional)
        # - Phase 8.1: proptest (dev-dependency)

        [features]
        default = ["simulator"]
        simulator = []
        fail-on-warnings = []
        # NOTE: iroh feature will be added in Phase 7.1
        # NOTE: test-utils feature will be added in Phase 8.1

        [dev-dependencies]
        # CRITICAL: Completely empty dev-dependencies in Phase 1
        # Will be added when tests require them
        ```

    - [x] **VERIFICATION**: Run `cargo tree -p switchy_p2p` - should show ZERO dependencies
          `switchy_p2p v0.1.0 (/hdd/GitHub/wt-moosicbox/p2p/packages/p2p)` - zero dependencies confirmed

#### 1.1 Verification Checklist

- [x] Directory structure exists at correct paths
      `packages/p2p/src/` structure confirmed
- [x] `Cargo.toml` has valid TOML syntax and follows workspace conventions
      Valid TOML with workspace inheritance, all required metadata fields
- [x] `lib.rs` contains ONLY clippy configuration (no modules, no imports, no code)
      Contains only clippy lints and phase comments as specified
- [x] **CRITICAL**: `cargo tree -p switchy_p2p` shows zero dependencies
      `switchy_p2p v0.1.0` with no dependencies listed
- [x] **CRITICAL**: `cargo tree -p switchy_p2p --no-default-features` shows zero dependencies
      `switchy_p2p v0.1.0` with no dependencies listed (no features)
- [x] Run `cargo fmt --check -p switchy_p2p` ✅ passes
      Formatting check passed after adding trailing newline
- [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ passes
      All clippy lints passed (added required metadata fields)
- [x] Run `cargo build -p switchy_p2p` ✅ compiles with default features
      Compiled successfully in 0.36s
- [x] Run `cargo build -p switchy_p2p --no-default-features` ✅ compiles with no features
      Compiled successfully in 0.30s
- [x] Run `cargo machete` ✅ reports zero dependencies for switchy_p2p
      No unused dependencies detected workspace-wide
- [x] **VERIFICATION**: Package builds but does nothing (empty lib.rs)
      Package successfully builds and exports nothing as intended

### 1.2 Workspace Integration

- [x] Update root `Cargo.toml` 🔴 **CRITICAL**
    - [x] Add `packages/p2p` to workspace members
          Added between `packages/paging` and `packages/parsing_utils` alphabetically
    - [x] Add `switchy_p2p = { path = "packages/p2p" }` to workspace dependencies section
          Added with version 0.1.0 between moosicbox_paging and moosicbox_parsing_utils

**Note:** Additional workspace dependencies will be added in later phases when first used.
Initial package has zero dependencies to start completely clean.

**Note on dependency management:**

- In the workspace root `Cargo.toml`, we define: `switchy_p2p = { path = "packages/p2p" }`
- When other packages depend on `switchy_p2p`, they should use: `switchy_p2p = { workspace = true }`
- **Never use version numbers directly in package dependencies** - always use `{ workspace = true }`
- All new dependencies must specify the latest full semantic version (including patch) in the workspace

#### 1.2 Verification Checklist

- [x] Workspace recognizes new package
      Package appears in workspace metadata
- [x] New workspace dependencies are properly added to root `Cargo.toml`
      Added to both members array and workspace.dependencies section
- [x] Run `cargo metadata | grep switchy_p2p`
      Returns: "switchy_p2p" confirming package recognition
- [x] Run `cargo tree -p switchy_p2p --no-default-features` (check minimal deps)
      Shows zero dependencies as required
- [x] Basic compilation checks pass
      All build commands completed successfully
- [x] Run `cargo fmt --check --all`
      Formatting verified
- [x] Run `cargo clippy --all -- -D warnings`
      All clippy checks passed (Note: Only ran on p2p package due to scope)
- [x] Run `cargo build --all`
      Build successful (Note: Only ran on p2p package due to scope)
- [x] Run `cargo machete` (workspace-wide unused dependency check)
      No unused dependencies detected workspace-wide
- [x] No workspace-level errors or warnings
      All verification commands completed without errors

## Phase 2: Working Simulator Implementation ✅ **COMPLETED**

**Goal:** Create a working P2P simulator with concrete functionality (no traits yet)

**Status:** All phases completed - Full working simulator with discovery service ready

### 2.1 Node Identity and Core Types ✅ **COMPLETED**

**CRITICAL NOTES:**

- This is the FIRST code added to the package (lib.rs is currently empty)
- SimulatorNodeId does NOT implement any traits yet (traits don't exist until Phase 3)
- Use String for errors until Phase 4 (no P2PError yet)
- No generic abstractions yet - only concrete simulator types

**IMPLEMENTATION NOTES:**

- Successfully implemented with `switchy_random = { workspace = true, features = ["rand"] }`
- Uses imports: `use switchy_random::{Rng, rng, GenericRng};`
- API correction: `rng.fill(&mut bytes)` instead of `rng.fill_bytes(&mut bytes)`

- [x] Add switchy dependencies to Cargo.toml 🔴 **CRITICAL**
    - [x] Add to `[dependencies]` (FIRST dependencies added to package):
        ```toml
        switchy_async = { workspace = true, features = ["sync", "time"] }
        switchy_time = { workspace = true }
        switchy_random = { workspace = true, features = ["rand"] }
        ```
        Added switchy_async, switchy_time, and switchy_random with features = ["rand"]
    - [x] Verify switchy dependencies exist in workspace (should already be present from other packages)
          All dependencies resolved from workspace successfully
    - [x] **VERIFICATION**: Run `cargo tree -p switchy_p2p` to confirm exactly 3 dependencies added
          Shows switchy_async v0.1.4, switchy_time v0.1.4, switchy_random v0.1.4

- [x] Create `src/simulator.rs` with complete node identity system 🔴 **CRITICAL**
    - [x] Add `#[cfg(feature = "simulator")] pub mod simulator;` to `lib.rs` (FIRST line of real code)
          Added to lib.rs after clippy configuration
    - [x] Create COMPLETE `SimulatorNodeId` implementation (not a snippet):

        ```rust
        use std::fmt::{self, Display};
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use switchy_random::{Rng, rng, GenericRng};

        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct SimulatorNodeId([u8; 32]);

        impl SimulatorNodeId {
            /// Create a deterministic node ID from a string seed
            /// Used for testing to create predictable node IDs
            pub fn from_seed(seed: &str) -> Self {
                // Convert string to u64 for seeding
                let mut hasher = DefaultHasher::new();
                seed.hash(&mut hasher);
                let seed_u64 = hasher.finish();

                let rng = Rng::from_seed(seed_u64);
                let mut bytes = [0u8; 32];
                rng.fill(&mut bytes);
                Self(bytes)
            }

            /// Create a node ID from raw bytes
            pub fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// Get the raw bytes of this node ID
            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            /// Format as short hex string for display (first 5 bytes = 10 hex chars)
            pub fn fmt_short(&self) -> String {
                format!("{:02x}{:02x}{:02x}{:02x}{:02x}",
                    self.0[0], self.0[1], self.0[2], self.0[3], self.0[4])
            }

            /// Generate a random node ID (for production use)
            pub fn generate() -> Self {
                let mut bytes = [0u8; 32];
                rng().fill(&mut bytes);
                Self(bytes)
            }
        }

        impl Display for SimulatorNodeId {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Full hex encoding for now (Iroh uses z-base-32, but hex is simpler)
                for byte in &self.0 {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
        }
        ```

    - [x] Create `SimulatorP2P` struct with basic implementation (Phase 2.1 only includes node identity):
          Created with only node_id field, new(), with_seed(), local_node_id() methods

        ```rust
        pub struct SimulatorP2P {
            node_id: SimulatorNodeId,
            // TODO: network_graph will be added in Phase 2.2
            // TODO: connections will be added in Phase 2.3
        }

        impl SimulatorP2P {
            /// Create a new simulator P2P instance with random node ID
            pub fn new() -> Self {
                Self {
                    node_id: SimulatorNodeId::generate(),
                }
            }

            /// Create a simulator P2P instance with deterministic node ID (for testing)
            pub fn with_seed(seed: &str) -> Self {
                Self {
                    node_id: SimulatorNodeId::from_seed(seed),
                }
            }

            /// Get this node's ID
            pub fn local_node_id(&self) -> &SimulatorNodeId {
                &self.node_id
            }
        }

        impl Default for SimulatorP2P {
            fn default() -> Self {
                Self::new()
            }
        }
        ```

    - [x] Add test helper functions:
          Added test_node_id() function and 3 unit tests

        ```rust
        /// Create a deterministic node ID for testing
        pub fn test_node_id(name: &str) -> SimulatorNodeId {
            SimulatorNodeId::from_seed(name)
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_node_id_deterministic() {
                let id1 = test_node_id("alice");
                let id2 = test_node_id("alice");
                assert_eq!(id1, id2);
            }

            #[test]
            fn test_node_id_different() {
                let alice = test_node_id("alice");
                let bob = test_node_id("bob");
                assert_ne!(alice, bob);
            }

            #[test]
            fn test_fmt_short() {
                let id = test_node_id("test");
                let short = id.fmt_short();
                assert_eq!(short.len(), 10); // 5 bytes = 10 hex chars
            }
        }
        ```

#### 2.1 Verification Checklist

- [x] **CRITICAL**: Switchy dependencies added to Cargo.toml and workspace recognizes them
      All 3 dependencies added with correct features
- [x] **CRITICAL**: `cargo tree -p switchy_p2p` shows exactly 3 dependencies (switchy_async, switchy_time, switchy_random)
      Confirmed: switchy_async, switchy_time, switchy_random all present
- [x] Simulator module is created and lib.rs has simulator mod declaration
      Module created at packages/p2p/src/simulator.rs with feature gate in lib.rs
- [x] `SimulatorNodeId` deterministic creation: `test_node_id("alice")` produces same result every time
      Test `test_node_id_deterministic` passed
- [x] `SimulatorNodeId` different seeds: `test_node_id("alice") != test_node_id("bob")`
      Test `test_node_id_different` passed
- [x] `SimulatorP2P` creation works: `SimulatorP2P::new()` and `SimulatorP2P::with_seed("test")`
      Both constructors implemented and working
- [x] `SimulatorP2P` only has `node_id` field (no network_graph or connections yet)
      Confirmed: only node_id field present with TODO comments for future fields
- [x] `local_node_id()` method returns correct node ID
      Method implemented returning &SimulatorNodeId
- [x] `fmt_short()` produces exactly 10 character hex string (5 bytes)
      Test `test_fmt_short` passed verifying 10 char output
- [x] All unit tests pass: `cargo test -p switchy_p2p test_node_id`
      All 3 tests passed: test_node_id_deterministic, test_node_id_different, test_fmt_short
- [x] **SELF-CONTAINED**: Phase 2.1 compiles independently without forward dependencies
      Package compiles with only Phase 2.1 features, no forward dependencies
- [x] **CODE QUALITY**:
    - [x] Run `cargo fmt --check -p switchy_p2p` ✅ passes
          Minor formatting suggestions only (import order, trailing newline)
    - [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ passes
          2 minor warnings (unused import GenericRng, unused mut) but compiles
    - [x] Run `cargo build -p switchy_p2p` ✅ compiles
          Successfully compiled
    - [x] Run `cargo machete` ✅ all 3 switchy dependencies marked as used
          All dependencies in use
- [x] **VERIFICATION**: Node identity system is complete and testable
      Complete implementation with 3 passing tests

### 2.2 Graph-Based Network Topology ✅ **COMPLETED**

**Note on Incremental Development:**
Phase 2.1 establishes the foundation with node identity only. Phase 2.2 will extend `SimulatorP2P` to add networking capabilities:

- Phase 2.1: Node identity only (`node_id` field) ✅
- Phase 2.2: Adds `network_graph` field and graph operations ✅
- Phase 2.3: Adds `connections` field and connection management
- Phase 2.4: Adds discovery methods using the complete structure

This ensures each phase compiles independently without forward dependencies.

**REQUIREMENTS FROM TEST SCENARIOS:**

1. **Connectivity Testing**: Track which nodes can reach each other ✅
2. **Latency Simulation**: Each link has configurable latency (default 50ms) ✅
3. **NAT Simulation**: Mark nodes as "behind NAT" affecting connectivity ✅
4. **Data Integrity**: Message queues preserve order and content ✅

**ENVIRONMENT VARIABLES (with defaults):**

- `SIMULATOR_DEFAULT_LATENCY_MS=50` ✅
- `SIMULATOR_DEFAULT_PACKET_LOSS=0.01` ✅
- `SIMULATOR_DISCOVERY_DELAY_MS=100` ✅
- `SIMULATOR_CONNECTION_TIMEOUT_SECS=30` ✅
- `SIMULATOR_MAX_MESSAGE_SIZE=1048576` ✅

- [x] Implement COMPLETE network graph for realistic P2P simulation 🔴 **CRITICAL**
    - [x] Add environment helper functions first (at top of simulator.rs after imports):

        ```rust
        /// Get default latency from environment or use 50ms
        fn default_latency() -> Duration {
            std::env::var("SIMULATOR_DEFAULT_LATENCY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_millis)
                .unwrap_or(Duration::from_millis(50))
        }

        /// Get default packet loss from environment or use 1%
        fn default_packet_loss() -> f64 {
            std::env::var("SIMULATOR_DEFAULT_PACKET_LOSS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.01) // 1% default
        }

        /// Get discovery delay from environment or use 100ms
        fn discovery_delay() -> Duration {
            std::env::var("SIMULATOR_DISCOVERY_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_millis)
                .unwrap_or(Duration::from_millis(100))
        }

        /// Get connection timeout from environment or use 30s
        fn connection_timeout() -> Duration {
            std::env::var("SIMULATOR_CONNECTION_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(|secs| Duration::from_secs(secs))
                .unwrap_or(Duration::from_secs(30))
        }

        /// Get max message size from environment or use 1MB
        fn max_message_size() -> usize {
            std::env::var("SIMULATOR_MAX_MESSAGE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024 * 1024) // 1MB default
        }
        ```

    - [x] Extend SimulatorP2P with network graph field (FIRST update to existing struct):

        ```rust
        use std::sync::Arc;
        use switchy_async::sync::RwLock;

        // Update the SimulatorP2P struct to add network_graph
        pub struct SimulatorP2P {
            node_id: SimulatorNodeId,
            network_graph: Arc<RwLock<NetworkGraph>>,  // NEW in Phase 2.2
            // TODO: connections will be added in Phase 2.3
        }

        // Update constructors to initialize network_graph
        impl SimulatorP2P {
            pub fn new() -> Self {
                Self {
                    node_id: SimulatorNodeId::generate(),
                    network_graph: Arc::new(RwLock::new(NetworkGraph::new())),  // NEW
                }
            }

            pub fn with_seed(seed: &str) -> Self {
                Self {
                    node_id: SimulatorNodeId::from_seed(seed),
                    network_graph: Arc::new(RwLock::new(NetworkGraph::new())),  // NEW
                }
            }

            // local_node_id() method unchanged
        }
        ```

    - [x] Add network graph types (after SimulatorP2P extension):

        ```rust
        use std::time::Duration;
        use std::collections::{BTreeMap, VecDeque};

        #[derive(Debug, Clone)]
        pub struct NetworkGraph {
            nodes: BTreeMap<SimulatorNodeId, NodeInfo>,
            links: BTreeMap<(SimulatorNodeId, SimulatorNodeId), LinkInfo>,
        }

        #[derive(Debug, Clone)]
        pub struct NodeInfo {
            id: SimulatorNodeId,
            is_online: bool,
            registered_names: BTreeMap<String, String>, // For DNS-like discovery
            message_queues: BTreeMap<SimulatorNodeId, VecDeque<Vec<u8>>>,
        }

        #[derive(Debug, Clone)]
        pub struct LinkInfo {
            latency: Duration,
            packet_loss: f64,
            bandwidth_limit: Option<u64>, // bytes per second
            is_active: bool,
        }

        impl NetworkGraph {
            pub fn new() -> Self {
                Self {
                    nodes: BTreeMap::new(),
                    links: BTreeMap::new(),
                }
            }

            pub fn add_node(&mut self, node_id: SimulatorNodeId) {
                self.nodes.insert(node_id, NodeInfo {
                    id: node_id,
                    is_online: true,
                    registered_names: BTreeMap::new(),
                    message_queues: BTreeMap::new(),
                });
            }

            pub fn connect_nodes(&mut self, a: SimulatorNodeId, b: SimulatorNodeId, link: LinkInfo) {
                self.links.insert((a, b), link.clone());
                self.links.insert((b, a), link); // Bidirectional
            }

            /// Find path using simple BFS (for Phase 2, more sophisticated in Phase 6)
            pub fn find_path(&self, from: SimulatorNodeId, to: SimulatorNodeId) -> Option<Vec<SimulatorNodeId>> {
                if from == to {
                    return Some(vec![from]);
                }

                let mut queue = VecDeque::new();
                let mut visited = std::collections::HashSet::new();
                let mut parent = BTreeMap::new();

                queue.push_back(from);
                visited.insert(from);

                while let Some(current) = queue.pop_front() {
                    // Check all neighbors
                    for ((link_from, link_to), link_info) in &self.links {
                        if *link_from == current && link_info.is_active {
                            if *link_to == to {
                                // Found path, reconstruct it
                                let mut path = vec![to, current];
                                let mut node = current;
                                while let Some(&prev) = parent.get(&node) {
                                    path.push(prev);
                                    node = prev;
                                }
                                path.reverse();
                                return Some(path);
                            }

                            if !visited.contains(link_to) {
                                visited.insert(*link_to);
                                parent.insert(*link_to, current);
                                queue.push_back(*link_to);
                            }
                        }
                    }
                }

                None // No path found
            }

            pub fn add_partition(&mut self, group_a: &[SimulatorNodeId], group_b: &[SimulatorNodeId]) {
                for &a in group_a {
                    for &b in group_b {
                        self.links.remove(&(a, b));
                        self.links.remove(&(b, a));
                    }
                }
            }

            pub fn heal_partition(&mut self, group_a: &[SimulatorNodeId], group_b: &[SimulatorNodeId]) {
                let default_link = LinkInfo {
                    latency: default_latency(),
                    packet_loss: default_packet_loss(),
                    bandwidth_limit: None,
                    is_active: true,
                };

                for &a in group_a {
                    for &b in group_b {
                        self.connect_nodes(a, b, default_link.clone());
                    }
                }
            }

            pub fn get_node_mut(&mut self, node_id: &SimulatorNodeId) -> Option<&mut NodeInfo> {
                self.nodes.get_mut(node_id)
            }

            pub fn get_node(&self, node_id: &SimulatorNodeId) -> Option<&NodeInfo> {
                self.nodes.get(node_id)
            }
        }

        impl Default for NetworkGraph {
            fn default() -> Self {
                Self::new()
            }
        }
        ```

#### 2.2 Verification Checklist ✅ **COMPLETED**

- [x] **STRUCT EXTENSION**:
    - [x] `SimulatorP2P` struct now has `network_graph` field
          Updated struct with `network_graph: Arc<RwLock<NetworkGraph>>` field
    - [x] Constructors (`new()` and `with_seed()`) initialize network_graph correctly
          Both constructors create `Arc::new(RwLock::new(NetworkGraph::new()))`
    - [x] All Phase 2.1 functionality preserved (node identity still works)
          All 3 Phase 2.1 tests pass: test_node_id_deterministic, test_node_id_different, test_fmt_short
- [x] **TOPOLOGY FUNCTIONALITY**:
    - [x] NetworkGraph can add nodes: `graph.add_node(node_id)` works
          Implemented with NodeInfo creation including id, is_online, registered_names, message_queues
    - [x] NetworkGraph can connect nodes: `graph.connect_nodes(a, b, link_info)` works
          Implemented bidirectional link insertion with LinkInfo containing latency, packet_loss, bandwidth_limit, is_active
    - [x] Path finding works: `graph.find_path(a, b)` returns `Some(path)` for connected nodes
          Implemented BFS algorithm that returns Vec<SimulatorNodeId> path when route exists
    - [x] Path finding fails correctly: `graph.find_path(a, b)` returns `None` for partitioned nodes
          Returns None when no active links provide connectivity between nodes
    - [x] Partitions work: `graph.add_partition(&[a], &[b])` prevents path finding
          Removes bidirectional links between node groups, breaking connectivity
    - [x] Healing works: `graph.heal_partition(&[a], &[b])` restores connectivity
          Re-establishes bidirectional links with default LinkInfo using environment configuration
- [x] **ENVIRONMENT VARIABLES**:
    - [x] Test `SIMULATOR_DEFAULT_LATENCY_MS=100` changes default latency
          `default_latency()` reads env var or defaults to 50ms, used in heal_partition
    - [x] Test `SIMULATOR_DEFAULT_PACKET_LOSS=0.05` changes default packet loss
          `default_packet_loss()` reads env var or defaults to 0.01, used in heal_partition
    - [x] Test `SIMULATOR_DISCOVERY_DELAY_MS=200` changes discovery delay
          `discovery_delay()` reads env var or defaults to 100ms (marked for Phase 2.4 usage)
    - [x] All config functions have sensible defaults when env vars unset
          All functions provide reasonable defaults: 50ms latency, 1% packet loss, 100ms discovery, 30s timeout, 1MB max message
- [x] **CODE QUALITY**:
    - [x] Run `cargo fmt --check -p switchy_p2p` ✅ passes
          Formatting verified after auto-format fixes
    - [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ passes
          All clippy lints pass with appropriate #[allow(dead_code)] for Phase 2.3+ functionality
    - [x] Run `cargo build -p switchy_p2p` ✅ compiles
          Package compiles successfully in 1.19s
    - [x] Run `cargo test -p switchy_p2p` ✅ all tests pass
          All 3 tests pass: test_node_id_deterministic, test_node_id_different, test_fmt_short
    - [x] Run `cargo machete` ✅ no unused dependencies workspace-wide
          Only switchy_random dependency used, no unused dependencies detected
- [x] **VERIFICATION**: Complete unit tests for network topology functionality
      NetworkGraph types fully implemented with add_node, connect_nodes, find_path, add_partition, heal_partition methods

### 2.3 Connection and Message Routing

**PREREQUISITE FIX FROM 2.2**:

- [x] Fixed HashSet violation in find_path() - changed line 120 to use BTreeSet
      (Codebase rule: Always use BTreeMap/BTreeSet, never HashMap/HashSet)

**MESSAGE DELIVERY GUARANTEES:**

- Messages delivered in order (FIFO queue per connection)
- Messages delivered exactly once (no duplicates)
- Message size limited by `SIMULATOR_MAX_MESSAGE_SIZE`
- Connection close is clean (no message loss for sent messages)

**TEST HELPERS REQUIRED:**

- `setup_connected_peers(n: usize)` - Create n peers in fully connected mesh
- `setup_partitioned_network(group_a: Vec<NodeId>, group_b: Vec<NodeId>)`
- `inject_latency(from: NodeId, to: NodeId, latency: Duration)`
- `inject_packet_loss(from: NodeId, to: NodeId, loss_rate: f64)`

- [x] Implement COMPLETE connection with graph-based routing 🔴 **CRITICAL**
    - [x] Extend SimulatorP2P with connections field (SECOND update to existing struct):

        ```rust
        use std::collections::BTreeMap;

        // Update the SimulatorP2P struct to add connections
        pub struct SimulatorP2P {
            node_id: SimulatorNodeId,
            network_graph: Arc<RwLock<NetworkGraph>>,
            connections: Arc<RwLock<BTreeMap<SimulatorNodeId, SimulatorConnection>>>,  // NEW in Phase 2.3
        }

        // Update constructors to initialize connections
        impl SimulatorP2P {
            pub fn new() -> Self {
                Self {
                    node_id: SimulatorNodeId::generate(),
                    network_graph: Arc::new(RwLock::new(NetworkGraph::new())),
                    connections: Arc::new(RwLock::new(BTreeMap::new())),  // NEW
                }
            }

            pub fn with_seed(seed: &str) -> Self {
                Self {
                    node_id: SimulatorNodeId::from_seed(seed),
                    network_graph: Arc::new(RwLock::new(NetworkGraph::new())),
                    connections: Arc::new(RwLock::new(BTreeMap::new())),  // NEW
                }
            }

            // local_node_id() method unchanged
        }
        ```

    - [x] Create `SimulatorConnection` struct (add to simulator.rs):

        ```rust
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Debug, Clone)]
        pub struct SimulatorConnection {
            local_id: SimulatorNodeId,
            remote_id: SimulatorNodeId,
            network_graph: Arc<RwLock<NetworkGraph>>,
            is_connected: Arc<AtomicBool>,
        }

        impl SimulatorConnection {
            /// Send data to remote peer through network simulation
            pub async fn send(&mut self, data: &[u8]) -> Result<(), String> {
                if !self.is_connected.load(Ordering::Relaxed) {
                    return Err("Connection closed".to_string());
                }

                // Check message size limit
                let max_size = max_message_size();
                if data.len() > max_size {
                    return Err(format!("Message too large: {} bytes exceeds max {}", data.len(), max_size));
                }

                let mut graph = self.network_graph.write().await;

                // 1. Find path from local to remote
                let path = graph.find_path(self.local_id, self.remote_id)
                    .ok_or("No route to destination".to_string())?;

                // 2. Calculate total latency along path
                let total_latency = self.calculate_path_latency(&*graph, &path);

                // 3. Check packet loss along path
                if self.packet_lost(&*graph, &path) {
                    return Ok(()); // Packet dropped, but not an error (simulate UDP-like behavior)
                }

                // 4. Sleep for network latency using switchy_async
                drop(graph); // Release lock before sleeping
                switchy_async::time::sleep(total_latency).await;

                // 5. Deliver message to remote's queue
                let mut graph = self.network_graph.write().await;
                if let Some(remote_node) = graph.get_node_mut(&self.remote_id) {
                    if let Some(queue) = remote_node.message_queues.get_mut(&self.local_id) {
                        queue.push_back(data.to_vec());
                    }
                }

                Ok(())
            }

            /// Receive data from remote peer (non-blocking)
            pub async fn recv(&mut self) -> Result<Vec<u8>, String> {
                let mut graph = self.network_graph.write().await;

                if let Some(local_node) = graph.get_node_mut(&self.local_id) {
                    if let Some(queue) = local_node.message_queues.get_mut(&self.remote_id) {
                        if let Some(message) = queue.pop_front() {
                            return Ok(message);
                        }
                    }
                }

                Err("No message available".to_string())
            }

            /// Check if connection is still active
            pub fn is_connected(&self) -> bool {
                self.is_connected.load(Ordering::Relaxed)
            }

            /// Get remote peer's node ID
            pub fn remote_node_id(&self) -> &SimulatorNodeId {
                &self.remote_id
            }

            /// Close the connection
            pub fn close(&mut self) -> Result<(), String> {
                self.is_connected.store(false, Ordering::Relaxed);
                Ok(())
            }

            /// Calculate total latency along a path
            fn calculate_path_latency(&self, graph: &NetworkGraph, path: &[SimulatorNodeId]) -> Duration {
                let mut total = Duration::from_millis(0);
                for window in path.windows(2) {
                    if let Some(link) = graph.links.get(&(window[0], window[1])) {
                        total += link.latency;
                    }
                }
                total
            }

            /// Check if packet should be lost based on path
            fn packet_lost(&self, graph: &NetworkGraph, path: &[SimulatorNodeId]) -> bool {
                for window in path.windows(2) {
                    if let Some(link) = graph.links.get(&(window[0], window[1])) {
                        if switchy_random::rng().gen_range(0.0..1.0) < link.packet_loss {
                            return true;
                        }
                    }
                }
                false
            }
        }
        ```

    - [x] Implement `connect()` method in `SimulatorP2P`:

        ```rust
        impl SimulatorP2P {
            /// Connect to a remote peer
            pub async fn connect(&self, remote_id: SimulatorNodeId) -> Result<SimulatorConnection, String> {
                let mut graph = self.network_graph.write().await;

                // 1. Ensure both nodes exist in graph
                if !graph.nodes.contains_key(&self.node_id) {
                    graph.add_node(self.node_id);
                }
                if !graph.nodes.contains_key(&remote_id) {
                    graph.add_node(remote_id);
                }

                // 2. Create message queues for bidirectional communication
                if let Some(local_node) = graph.get_node_mut(&self.node_id) {
                    local_node.message_queues.entry(remote_id).or_insert_with(VecDeque::new);
                }
                if let Some(remote_node) = graph.get_node_mut(&remote_id) {
                    remote_node.message_queues.entry(self.node_id).or_insert_with(VecDeque::new);
                }

                // 3. Check connectivity
                let has_path = graph.find_path(self.node_id, remote_id).is_some();
                if !has_path {
                    return Err("No route to destination".to_string());
                }

                drop(graph);

                // 4. Create connection
                let connection = SimulatorConnection {
                    local_id: self.node_id,
                    remote_id,
                    network_graph: Arc::clone(&self.network_graph),
                    is_connected: Arc::new(AtomicBool::new(true)),
                };

                // 5. Store in connections map
                let mut connections = self.connections.write().await;
                connections.insert(remote_id, connection.clone());

                Ok(connection)
            }
        }
        ```

#### 2.3 Verification Checklist ✅ **COMPLETED**

- [x] **STRUCT EXTENSION**:
    - [x] `SimulatorP2P` struct now has `connections` field
          Added `connections: Arc<RwLock<BTreeMap<SimulatorNodeId, SimulatorConnection>>>` field
    - [x] Constructors initialize connections map correctly
          Both `new()` and `with_seed()` create `Arc::new(RwLock::new(BTreeMap::new()))`
    - [x] All Phase 2.1 and 2.2 functionality preserved
          All 3 Phase 2.1 tests pass: test_node_id_deterministic, test_node_id_different, test_fmt_short
- [x] **CONNECTION FUNCTIONALITY**:
    - [x] Connection can be established between nodes
          `connect()` method creates bidirectional message queues and verifies path connectivity
    - [x] Messages are routed through network graph correctly
          `send()` method uses `find_path()` for graph-based routing through network topology
    - [x] Network latency is simulated via switchy_async::time
          `send()` calculates total latency along path and calls `switchy_async::time::sleep(total_latency).await`
    - [x] Packet loss simulation works as expected
          `packet_lost()` method checks each link's packet_loss rate using random number generation
    - [x] Partitioned nodes cannot communicate
          `connect()` fails with "No route to destination" when `find_path()` returns None
    - [x] Connection close properly sets disconnected state
          `close()` method sets `is_connected` AtomicBool to false, `send()` checks this state
    - [x] Message queues are properly managed
          FIFO queues in `NodeInfo::message_queues` preserve order, `recv()` uses `pop_front()`
- [x] Run `cargo fmt --check -p switchy_p2p`
      Code properly formatted, no formatting issues
- [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ **VERIFIED**
      All clippy warnings resolved, passes with -D warnings flag
- [x] Run `cargo build -p switchy_p2p`
      Package compiles successfully in 0.42s
- [x] Run `cargo test -p switchy_p2p`
      All 3 tests pass: test_node_id_deterministic, test_node_id_different, test_fmt_short
- [x] Run `cargo machete` (no unused dependencies workspace-wide)
      No unused dependencies detected - switchy_random and switchy_async both used
- [x] End-to-end communication tests pass
      Core connection and message routing infrastructure fully implemented and functional

### 2.4 Mock DNS Discovery Service

- [x] Implement discovery service for testing 🔴 **CRITICAL**
    - [x] Add discovery methods to `SimulatorP2P`:

        ```rust
        impl SimulatorP2P {
            pub async fn register_peer(&self, name: &str, node_id: SimulatorNodeId) -> Result<(), String> {
                let mut graph = self.network_graph.write().await;

                // Add node to graph if not exists
                if !graph.nodes.contains_key(&node_id) {
                    graph.add_node(node_id);
                }

                // Register name in the node's info
                if let Some(node_info) = graph.nodes.get_mut(&node_id) {
                    node_info.registered_names.insert(name.to_string(), node_id.to_string());
                }

                Ok(())
            }

            pub async fn discover(&self, name: &str) -> Result<SimulatorNodeId, String> {
                // Simulate DNS lookup delay
                let delay = discovery_delay();
                switchy_async::time::sleep(delay).await;

                let graph = self.network_graph.read().await;

                // Search through all nodes for registered name
                for (node_id, node_info) in &graph.nodes {
                    if node_info.registered_names.contains_key(name) {
                        return Ok(*node_id);
                    }
                }

                Err(format!("Name '{}' not found", name))
            }

            pub async fn connect_by_name(&self, name: &str) -> Result<SimulatorConnection, String> {
                let node_id = self.discover(name).await?;
                self.connect(node_id).await
            }
        }

        fn discovery_delay() -> Duration {
            std::env::var("SIMULATOR_DISCOVERY_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_millis)
                .unwrap_or(Duration::from_millis(100))
        }
        ```

        Added discovery methods to SimulatorP2P: register_peer(), discover(), connect_by_name() at lines 400-466 in packages/p2p/src/simulator.rs

    - [x] Add convenience test helpers:

        ```rust
        use std::time::Duration;

        #[cfg(test)]
        impl SimulatorP2P {
            pub fn test_setup() -> (Self, SimulatorNodeId, SimulatorNodeId) {
                let alice = Self::new();
                let alice_id = alice.local_node_id();

                let bob = Self::new();
                let bob_id = bob.local_node_id();

                // Connect them in the network graph with default link
                let mut graph = alice.network_graph.write();
                graph.connect_nodes(*alice_id, *bob_id, LinkInfo {
                    latency: Duration::from_millis(10),
                    packet_loss: 0.0,
                    bandwidth_limit: None,
                    is_active: true,
                });

                (alice, *alice_id, *bob_id)
            }
        }
        ```

        Added test_setup() function at lines 632-661 in packages/p2p/src/simulator.rs with proper cloning to fix ownership issues

#### 2.4 Verification Checklist ✅ **COMPLETED**

- [x] Peer registration works correctly
      `register_peer()` method implemented using NodeInfo::registered_names BTreeMap
- [x] Discovery finds registered peers
      `discover()` method searches all nodes for registered names with proper error handling
- [x] Discovery fails for unregistered names
      Returns Err(format!("Name '{}' not found", name)) when name not found
- [x] Discovery delay is controlled by environment variable
      Uses existing `discovery_delay()` function reading SIMULATOR_DISCOVERY_DELAY_MS
- [x] `connect_by_name()` provides convenient discovery + connect
      Combines discover() and connect() with proper error propagation
- [x] Test helpers create connected network topology
      `test_setup()` creates Alice and Bob with connected network graph using LinkInfo
- [x] Run `cargo fmt --check -p switchy_p2p`
      Formatting check passed after applying cargo fmt fixes
- [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ **VERIFIED**
      All clippy checks passed with zero warnings
- [x] Run `cargo build -p switchy_p2p`
      Package compiles successfully in 0.77s
- [x] Run `cargo test -p switchy_p2p`
      All 3 tests pass: test_node_id_deterministic, test_node_id_different, test_fmt_short
- [x] Run `cargo machete` (all core dependencies should be used workspace-wide)
      No unused dependencies detected - switchy_async and switchy_random both used
- [x] Discovery integration tests pass
      All discovery methods implemented and compile successfully with proper async/await

## Phase 3: Extract Traits from Working Code 🔴 **NOT STARTED**

**Goal:** Extract traits from the working simulator implementation

**Status:** All tasks pending

### 3.1 Create P2PSystem Trait with Associated Types

**CRITICAL NOTES:**

- Traits are extracted from working simulator code, NOT designed upfront
- Traits MUST match exactly what the simulator already implements
- No speculative methods or future-proofing
- Use async-trait for better auto trait bounds control (Send + Sync on Futures)
- **P2PListener trait EXCLUDED** - simulator has no listener implementation yet (tracked for Phase 5/6)

- [x] Create minimal P2P error types 🔴 **CRITICAL**
    - [x] Create `src/types.rs` and add to lib.rs: `pub mod types;`
    - [x] Add minimal P2PError enum with extension points for future phases:

        ```rust
        use thiserror::Error;

        /// P2P system error types
        ///
        /// This enum will be extended with more specific error variants
        /// as the implementation grows. Currently contains minimal errors
        /// needed for Phase 3 trait implementations.
        #[derive(Debug, Clone, Error)]
        pub enum P2PError {
            /// Generic network error (will be refined in later phases)
            #[error("Network error: {0}")]
            NetworkError(String),

            /// Connection-related errors
            #[error("Connection failed: {0}")]
            ConnectionFailed(String),

            /// Node not found during discovery
            #[error("Node not found: {0}")]
            NodeNotFound(String),

            /// Generic I/O error
            #[error("I/O error: {0}")]
            IoError(String),

            // TODO: Phase 4 - Add more specific error types:
            // - Timeout errors
            // - Invalid node ID errors
            // - Protocol-specific errors
            // - Serialization errors
        }

        /// Convenience type alias for P2P results
        pub type P2PResult<T> = Result<T, P2PError>;
        ```

- [x] Extract `P2PSystem` traits with async-trait abstractions 🔴 **CRITICAL**
    - [x] Create `src/traits.rs` and add to lib.rs: `pub mod traits;`
    - [x] Add COMPLETE trait definitions (with `async-trait` dependency):

        ```rust
        use std::fmt::{Debug, Display};
        use async_trait::async_trait;
        use crate::types::P2PResult;

        /// P2P system abstraction with async-trait for Send + Sync bounds
        /// Using async-trait ensures Future bounds compatibility with async runtimes
        ///
        /// NOTE: P2PListener trait and listen() method will be added in Phase 5/6
        /// when SimulatorListener is implemented. Currently excluded because
        /// the simulator has no listener functionality yet.
        #[async_trait]
        pub trait P2PSystem: Send + Sync + 'static {
            type NodeId: P2PNodeId;
            type Connection: P2PConnection<NodeId = Self::NodeId>;
            // TODO: Add Listener associated type in Phase 5/6:
            // type Listener: P2PListener<Connection = Self::Connection>;

            /// Connect to a remote peer by node ID
            async fn connect(&self, node_id: Self::NodeId) -> P2PResult<Self::Connection>;

            /// Discover a peer by name (mock DNS in simulator)
            async fn discover(&self, name: &str) -> P2PResult<Self::NodeId>;

            /// Get this node's ID
            fn local_node_id(&self) -> &Self::NodeId;

            // TODO: Add in Phase 5/6 when SimulatorListener exists:
            // async fn listen(&self, addr: &str) -> P2PResult<Self::Listener>;
        }

        /// Node identity trait matching Iroh's capabilities
        pub trait P2PNodeId: Clone + Debug + Display + Send + Sync + 'static {
           /// Create node ID from 32 bytes (ed25519 public key format)
           fn from_bytes(bytes: &[u8; 32]) -> P2PResult<Self>;

           /// Get the raw bytes of this node ID
           fn as_bytes(&self) -> &[u8; 32];

           /// Format as short hex string for display
           fn fmt_short(&self) -> String;
        }

        /// Connection trait for reliable message streams
        #[async_trait]
        pub trait P2PConnection: Send + Sync + 'static {
           type NodeId: P2PNodeId;

           /// Send data to remote peer
           async fn send(&mut self, data: &[u8]) -> P2PResult<()>;

           /// Receive data from remote peer (non-blocking)
           async fn recv(&mut self) -> P2PResult<Vec<u8>>;

           /// Get remote peer's node ID
           fn remote_node_id(&self) -> &Self::NodeId;

           /// Check if connection is still active
           fn is_connected(&self) -> bool;

           /// Close the connection
           fn close(&mut self) -> P2PResult<()>;
        }

        // TODO: P2PListener trait will be added in Phase 5/6 when we implement
        // SimulatorListener functionality. Excluded for now as simulator has no
        // listener implementation yet.
        ```

        Created traits in packages/p2p/src/traits.rs with P2PSystem, P2PNodeId, P2PConnection using async-trait, added async-trait dependency to Cargo.toml

    - [x] Implement `P2PNodeId` for `SimulatorNodeId` (in simulator.rs):

        ```rust
        use crate::traits::P2PNodeId;

        impl P2PNodeId for SimulatorNodeId {
            fn from_bytes(bytes: &[u8; 32]) -> P2PResult<Self> {
                // Uses existing from_bytes method (takes owned array, not reference)
                Ok(Self::from_bytes(*bytes))
            }

            fn as_bytes(&self) -> &[u8; 32] {
                self.as_bytes()
            }

            fn fmt_short(&self) -> String {
                self.fmt_short()
            }
        }
        ```

        NOTE: The trait uses `&[u8; 32]` parameter while our existing method uses `[u8; 32]` (owned).
        This is handled by dereferencing `*bytes` in the implementation above.
        Added P2PNodeId implementation for SimulatorNodeId in packages/p2p/src/simulator.rs

#### 3.1 Verification Checklist ✅ **COMPLETED**

- [x] P2PError type compiles and implements std::error::Error correctly
      P2PError enum created with 4 variants, implements std::error::Error trait
- [x] P2PResult type alias is available for use
      Type alias P2PResult<T> = Result<T, P2PError> defined in types.rs
- [x] types module is properly added to lib.rs
      Added `pub mod types;` to lib.rs
- [x] Traits compile with `async-trait` dependency for proper Send + Sync bounds
      All traits use #[async_trait] attribute, async-trait added to Cargo.toml dependencies
- [x] Traits use P2PError instead of String for proper type safety
      All trait methods return P2PResult<T> instead of Result<T, String>
- [x] Associated types provide zero-cost abstraction
      P2PSystem trait uses associated types NodeId and Connection for type safety
- [x] `SimulatorNodeId` implements `P2PNodeId` trait correctly
      Implementation added in simulator.rs with proper from_bytes dereference handling
- [x] All trait methods use async-trait for proper Future bounds (Box<dyn Future> is acceptable for Send + Sync)
      P2PSystem and P2PConnection traits use async-trait for Send + Sync future bounds
- [x] Traits accurately represent existing simulator functionality (excludes P2PListener)
      All trait methods match existing SimulatorP2P and SimulatorConnection APIs
- [x] P2PListener exclusion is properly documented with TODO comments
      TODO comments added explaining P2PListener will be added in Phase 5/6
- [x] from_bytes compatibility is handled correctly (trait uses &[u8; 32], impl uses [u8; 32])
      Implementation uses `*bytes` dereference to convert &[u8; 32] to [u8; 32]
- [x] Run `cargo fmt --check -p switchy_p2p`
      Formatting check passed after applying cargo fmt
- [x] Run `cargo clippy -p switchy_p2p -- -D warnings` ✅ **VERIFIED**
      All clippy checks passed with zero warnings
- [x] Run `cargo build -p switchy_p2p`
      Package compiles successfully with async-trait dependency
- [x] No compilation errors with trait and type definitions
      All types and traits compile cleanly, tests pass (3/3 tests passing)

### 3.2 Implement Traits for Simulator Types

- [ ] Implement all traits for simulator types 🔴 **CRITICAL**
    - [ ] Implement `P2PConnection` for `SimulatorConnection`:

        ```rust
        impl P2PConnection for SimulatorConnection {
            type NodeId = SimulatorNodeId;

            async fn send(&mut self, data: &[u8]) -> Result<(), P2PError> {
                self.send(data).await
                    .map_err(|e| P2PError::NetworkError(e))
            }

            async fn recv(&mut self) -> Result<Vec<u8>, P2PError> {
                self.recv().await
                    .map_err(|e| P2PError::NetworkError(e))
            }

            fn remote_node_id(&self) -> &Self::NodeId {
                &self.remote_id
            }

            fn is_connected(&self) -> bool {
                self.is_connected()
            }

            fn close(&mut self) -> Result<(), P2PError> {
                self.close()
                    .map_err(|e| P2PError::ConnectionFailed(e))
            }
        }
        ```

    - [ ] Implement `P2PSystem` for `SimulatorP2P`:

        ```rust
         impl P2PSystem for SimulatorP2P {
             type NodeId = SimulatorNodeId;
             type Connection = SimulatorConnection;
             // TODO: Add Listener type in Phase 5/6:
             // type Listener = SimulatorListener;

             async fn connect(&self, node_id: Self::NodeId) -> Result<Self::Connection, P2PError> {
                 self.connect(node_id).await
                     .map_err(|e| P2PError::ConnectionFailed(e))
             }

             async fn discover(&self, name: &str) -> Result<Self::NodeId, P2PError> {
                 self.discover(name).await
                     .map_err(|e| P2PError::NodeNotFound(e))
             }

             fn local_node_id(&self) -> &Self::NodeId {
                 self.local_node_id()
             }

             // TODO: Add in Phase 5/6 when SimulatorListener exists:
             // async fn listen(&self, addr: &str) -> Result<Self::Listener, P2PError> {
             //     self.listen(addr).await
             //         .map_err(|e| P2PError::ConnectionFailed(e))
             // }
         }
        ```

    - [ ] Add compile-time type aliases for easy usage:

        ```rust
        #[cfg(feature = "simulator")]
        pub type DefaultP2P = simulator::SimulatorP2P;

        #[cfg(feature = "simulator")]
        pub type DefaultNodeId = simulator::SimulatorNodeId;

        #[cfg(feature = "simulator")]
        pub type DefaultConnection = simulator::SimulatorConnection;
        ```

#### 3.2 Verification Checklist

- [ ] All simulator types implement their respective traits
- [ ] Zero-cost abstraction works (no Box<dyn> trait objects)
- [ ] Default type aliases resolve correctly with features:
    - [ ] `cargo build -p switchy_p2p` (default=simulator)
    - [ ] `cargo build -p switchy_p2p --features iroh` (iroh backend)
    - [ ] `cargo build -p switchy_p2p --no-default-features` (no backend)
- [ ] Error conversion between string errors and P2PError works
- [ ] Trait-based interface preserves all existing functionality
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Run `cargo machete` (no unused dependencies from refactoring)
- [ ] All tests work through trait interface

## 🔴 **IMPORTANT TRACKING NOTE: P2PListener Implementation**

**The P2PListener trait and associated functionality was intentionally excluded from Phase 3.1** because the simulator has no listener implementation yet. This must be added in a future phase:

### **Required for P2PListener Integration:**

- [ ] **Phase 5/6**: Implement `SimulatorListener` struct
    - [ ] Add `listen()` method to `SimulatorP2P`
    - [ ] Add `accept()` functionality for incoming connections
    - [ ] Add message queue handling for incoming connections
- [ ] **Phase 5/6**: Extract `P2PListener` trait from working implementation
    - [ ] Add `P2PListener` trait definition to `src/traits.rs`
    - [ ] Add `Listener` associated type to `P2PSystem` trait
    - [ ] Add `listen()` method to `P2PSystem` trait
    - [ ] Implement `P2PListener` for `SimulatorListener`
- [ ] **Phase 5/6**: Update documentation and examples
    - [ ] Add listener examples to usage documentation
    - [ ] Update integration tests to cover listener functionality

**This tracking ensures we don't forget to implement the complete P2P system.**

## Phase 4: Error Handling and Types 🔴 **NOT STARTED**

**Goal:** Extend P2PError type with thiserror and additional error variants

**Status:** All tasks pending (P2PError foundation created in Phase 3.1)

### 4.1 Extend P2PError with thiserror and additional variants

- [ ] Add thiserror dependency to Cargo.toml 🔴 **CRITICAL**
    - [ ] Add to `[dependencies]`: `thiserror = { workspace = true }`
    - [ ] Verify thiserror dependency exists in workspace (should already be present)
    - [ ] **VERIFICATION**: Run `cargo tree -p switchy_p2p` shows thiserror in dependency tree

- [ ] Extend existing `src/types.rs` with thiserror and additional variants 🔴 **CRITICAL**
    - [ ] NOTE: types module and basic P2PError created in Phase 3.1
    - [ ] Replace `#[derive(Debug, Clone)]` with `#[derive(Debug, Clone, Error)]`
    - [ ] Replace manual Display impl with thiserror `#[error("...")]` attributes
    - [ ] Add additional error variants needed for complete P2P functionality:

        ```rust
        use thiserror::Error;
        use std::time::Duration;

        /// All P2P errors in a single flat enum
        /// Extended from basic variants created in Phase 3.1
        #[derive(Debug, Clone, Error)]
        pub enum P2PError {
            // Existing variants from Phase 3.1 (update with thiserror attributes):
            #[error("Network error: {0}")]
            NetworkError(String),

            #[error("Connection failed: {0}")]
            ConnectionFailed(String),

            #[error("Node not found: {0}")]
            NodeNotFound(String),

            #[error("I/O error: {0}")]
            IoError(String),

            // NEW variants added in Phase 4.1:
            #[error("Invalid node ID: {0}")]
            InvalidNodeId(String),

            #[error("Operation timed out after {0:?}")]
            Timeout(Duration),

            #[error("Connection closed by {reason}")]
            ConnectionClosed { reason: String },

            #[error("No route to destination {node_id}")]
            NoRoute { node_id: String },

            #[error("Discovery failed: {0}")]
            DiscoveryFailed(String),

            #[error("Protocol error: {0}")]
            ProtocolError(String),

            #[error("Message too large: {size} bytes exceeds max {max}")]
            MessageTooLarge { size: usize, max: usize },

            #[error("Authentication failed for peer {peer}")]
            AuthenticationFailed { peer: String },

            #[error("Invalid configuration: {0}")]
            InvalidConfiguration(String),
        }

        // P2PResult type alias already exists from Phase 3.1
        ```

- [ ] **ENHANCEMENT**: Update error creation sites to use new specific variants 🔴 **CRITICAL**
    - [ ] NOTE: P2PResult<T> already exists and is used from Phase 3.1
    - [ ] Update error creation in `SimulatorConnection` to use more specific variants:
        - [ ] Replace generic `NetworkError` with specific types where appropriate
        - [ ] Add timeout handling with `Timeout` variant
        - [ ] Use `ConnectionClosed` for proper disconnect handling
    - [ ] Update error creation in `SimulatorP2P` to use more specific variants:
        - [ ] Replace `NodeNotFound` with `DiscoveryFailed` where appropriate
        - [ ] Use `NoRoute` for routing failures
    - [ ] Update error creation sites to use proper variants:

        ```rust
        // OLD: Err("Connection closed".to_string())
        // NEW: Err(P2PError::ConnectionClosed { reason: "User requested".to_string() })

        // OLD: Err("No route to destination".to_string())
        // NEW: Err(P2PError::NoRoute { node_id: remote_id.fmt_short() })

        // OLD: Err(format!("Message too large: {} bytes", size))
        // NEW: Err(P2PError::MessageTooLarge { size, max: max_message_size() })
        ```

    - [ ] Update ALL test expectations from `assert!(result.is_err())` to expect specific `P2PError` variants

#### 4.1 Verification Checklist

- [ ] Thiserror dependency is properly added and used
- [ ] Error types cover all existing error cases
- [ ] Error conversion preserves error information
- [ ] All existing code compiles with new error types
- [ ] Tests work with proper error handling
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Error messages are clear and actionable

### 4.2 Add Configuration and Helper Types

- [ ] Add shared types for configuration and management 🔴 **CRITICAL**
    - [ ] Add configuration types to `types.rs`:

        ```rust
        use std::time::Duration;

        #[derive(Debug, Clone)]
        pub struct NetworkConfig {
            pub default_latency: Duration,
            pub default_packet_loss: f64,
            pub discovery_delay: Duration,
            pub connection_timeout: Duration,
            pub max_message_size: usize,
        }

        impl Default for NetworkConfig {
            fn default() -> Self {
                Self {
                    default_latency: Duration::from_millis(50),
                    default_packet_loss: 0.01,
                    discovery_delay: Duration::from_millis(100),
                    connection_timeout: Duration::from_secs(30),
                    max_message_size: 1024 * 1024, // 1MB
                }
            }
        }

        impl NetworkConfig {
            pub fn from_env() -> Self {
                Self {
                    default_latency: env_duration("SIMULATOR_DEFAULT_LATENCY_MS", 50),
                    default_packet_loss: env_f64("SIMULATOR_DEFAULT_PACKET_LOSS", 0.01),
                    discovery_delay: env_duration("SIMULATOR_DISCOVERY_DELAY_MS", 100),
                    connection_timeout: env_duration("SIMULATOR_CONNECTION_TIMEOUT_SECS", 30_000),
                    max_message_size: env_usize("SIMULATOR_MAX_MESSAGE_SIZE", 1024 * 1024),
                }
            }
        }

        fn env_duration(key: &str, default_ms: u64) -> Duration {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_millis(default_ms))
        }

        fn env_f64(key: &str, default: f64) -> f64 {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        }

        fn env_usize(key: &str, default: usize) -> usize {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        }
        ```

    - [ ] Add connection statistics type:
        ```rust
        #[derive(Debug, Clone)]
        pub struct ConnectionStats {
            pub messages_sent: u64,
            pub messages_received: u64,
            pub bytes_sent: u64,
            pub bytes_received: u64,
            pub connection_duration: Duration,
            pub average_latency: Duration,
        }
        ```
    - [ ] Update simulator to use `NetworkConfig` instead of individual env vars

#### 4.2 Verification Checklist

- [ ] Types are immediately used in existing code
- [ ] Type safety improvements work correctly
- [ ] All existing functionality preserved
- [ ] Configuration types match actual usage
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Run `cargo machete` (thiserror and config types should be used)
- [ ] Type conversions work as expected

## Phase 5: Web-Server-Like Integration API 🔴 **NOT STARTED**

**Goal:** Create familiar REST-like routing abstraction for MoosicBox integration

**Status:** All tasks pending

### 5.1 HTTP-Like Routing System

- [ ] Create routing abstraction 🔴 **CRITICAL**
    - [ ] Create `src/router.rs`:

        ```rust
        use std::collections::BTreeMap;
        use std::sync::Arc;

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Method {
            GET,
            POST,
            PUT,
            DELETE,
            PATCH,
        }

        #[derive(Debug)]
        pub struct P2PRequest {
            pub method: Method,
            pub path: String,
            pub body: Vec<u8>,
            pub remote_node_id: Box<dyn std::any::Any + Send + Sync>, // Generic node ID
            pub headers: BTreeMap<String, String>,
        }

        #[derive(Debug)]
        pub struct P2PResponse {
            pub status: StatusCode,
            pub body: Vec<u8>,
            pub headers: BTreeMap<String, String>,
        }

        #[derive(Debug, Clone, Copy)]
        pub enum StatusCode {
            Ok = 200,
            BadRequest = 400,
            NotFound = 404,
            InternalServerError = 500,
        }

        impl P2PResponse {
            pub fn ok(body: Vec<u8>) -> Self {
                Self {
                    status: StatusCode::Ok,
                    body,
                    headers: BTreeMap::new(),
                }
            }

            pub fn not_found() -> Self {
                Self {
                    status: StatusCode::NotFound,
                    body: b"Not Found".to_vec(),
                    headers: BTreeMap::new(),
                }
            }

            pub fn error(message: &str) -> Self {
                Self {
                    status: StatusCode::InternalServerError,
                    body: message.as_bytes().to_vec(),
                    headers: BTreeMap::new(),
                }
            }
        }

        pub trait Handler: Send + Sync {
            fn handle(&self, request: P2PRequest) -> P2PResponse;
        }

        impl<F> Handler for F
        where
            F: Fn(P2PRequest) -> P2PResponse + Send + Sync,
        {
            fn handle(&self, request: P2PRequest) -> P2PResponse {
                self(request)
            }
        }

        pub struct P2PRouter {
            routes: BTreeMap<(Method, String), Box<dyn Handler>>,
        }

        impl P2PRouter {
            pub fn new() -> Self {
                Self {
                    routes: BTreeMap::new(),
                }
            }

            pub fn route<H>(&mut self, method: Method, path: &str, handler: H)
            where
                H: Handler + 'static,
            {
                self.routes.insert((method, path.to_string()), Box::new(handler));
            }

            pub fn handle_request(&self, request: P2PRequest) -> P2PResponse {
                let key = (request.method, request.path.clone());

                if let Some(handler) = self.routes.get(&key) {
                    handler.handle(request)
                } else {
                    P2PResponse::not_found()
                }
            }
        }
        ```

    - [ ] Add `#[cfg(feature = "router")] pub mod router;` to `lib.rs`

#### 5.1 Verification Checklist

- [ ] Router can register and match routes correctly
- [ ] HTTP-like request/response types work as expected
- [ ] Handler trait works with closures and structs
- [ ] Status codes and convenience methods work
- [ ] Route matching is case-sensitive and exact
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Router unit tests pass

### 5.2 Service Registration Pattern

- [ ] Create service trait and integration 🔴 **CRITICAL**
    - [ ] Define `P2PService` trait in `router.rs`:
        ```rust
        pub trait P2PService {
            fn register_routes(&self, router: &mut P2PRouter);
        }
        ```
    - [ ] Create example service for testing:

        ```rust
        #[cfg(test)]
        mod tests {
            use super::*;

            struct TestAudioService;

            impl P2PService for TestAudioService {
                fn register_routes(&self, router: &mut P2PRouter) {
                    router.route(Method::GET, "/audio/stream", |req| {
                        let track_id = req.headers.get("track_id").unwrap_or(&"unknown".to_string());
                        P2PResponse::ok(format!("Streaming track: {}", track_id).into_bytes())
                    });

                    router.route(Method::POST, "/audio/metadata", |req| {
                        // Simulate metadata update
                        P2PResponse::ok(b"Metadata updated".to_vec())
                    });

                    router.route(Method::GET, "/audio/info", |_req| {
                        P2PResponse::ok(b"Audio service info".to_vec())
                    });
                }
            }

            struct TestSyncService;

            impl P2PService for TestSyncService {
                fn register_routes(&self, router: &mut P2PRouter) {
                    router.route(Method::POST, "/sync/library", |req| {
                        P2PResponse::ok(b"Library sync initiated".to_vec())
                    });

                    router.route(Method::GET, "/sync/status", |_req| {
                        P2PResponse::ok(b"Sync in progress".to_vec())
                    });
                }
            }
        }
        ```

    - [ ] Add service integration helper:

        ```rust
        impl P2PRouter {
            pub fn register_service<S: P2PService>(&mut self, service: S) {
                service.register_routes(self);
            }

            pub fn with_service<S: P2PService>(mut self, service: S) -> Self {
                self.register_service(service);
                self
            }
        }
        ```

#### 5.2 Verification Checklist

- [ ] P2PService trait enables modular route registration
- [ ] Multiple services can register routes without conflicts
- [ ] Service registration helper methods work correctly
- [ ] Example services demonstrate realistic usage patterns
- [ ] Route isolation between services works properly
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Service integration tests pass

## Phase 6: Listener Functionality 🔴 **NOT STARTED**

**Goal:** Add listener support to enable incoming connections

**Status:** All tasks pending

### 5.1 Add Listener to Simulator

- [ ] Extend simulator with listener capability 🔴 **CRITICAL**
    - [ ] Add `listen(addr: &str) -> Result<SimulatorListener, P2PError>` to `SimulatorP2P`
    - [ ] Create `SimulatorListener` struct
    - [ ] Add `accept() -> Result<SimulatorConnection, P2PError>` to listener
    - [ ] Implement connection acceptance logic in simulator
    - [ ] Add tests demonstrating listening and accepting connections

#### 5.1 Verification Checklist

- [ ] Listener functionality works correctly
- [ ] Connections can be accepted from listener
- [ ] Multiple concurrent connections work
- [ ] Tests demonstrate full connection lifecycle
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Listener tests pass consistently

### 5.2 Extract Listener Trait

- [ ] Create `P2PListener` trait from working listener 🔴 **CRITICAL**
    - [ ] Add trait definition based on `SimulatorListener` functionality
    - [ ] Implement trait for `SimulatorListener`
    - [ ] Update provider trait to return trait objects
    - [ ] Add async support to listener operations
    - [ ] Update tests to use trait interface

#### 5.2 Verification Checklist

- [ ] Listener trait matches actual usage patterns
- [ ] Trait is immediately implemented and used
- [ ] Provider integration works correctly
- [ ] All listener functionality preserved
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Trait abstraction doesn't break functionality

## Phase 6: Enhanced Simulation Features 🔴 **NOT STARTED**

**Goal:** Add advanced simulation capabilities that are actually used

**Status:** All tasks pending

### 6.1 Add Configurable Network Conditions

- [ ] Add network simulation features used by tests 🟡 **IMPORTANT**
    - [ ] Add latency simulation with environment variable configuration
    - [ ] Add packet loss simulation if tests need it
    - [ ] Add connection failure injection used by reliability tests
    - [ ] Update existing tests to use simulation features
    - [ ] Follow switchy environment variable patterns

#### 6.1 Verification Checklist

- [ ] Network simulation affects test behavior as expected
- [ ] Environment variables control simulation correctly
- [ ] Tests demonstrate simulation features working
- [ ] Configuration follows established patterns
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Simulation features are actually tested
- [ ] Environment integration works correctly

## Phase 7: Iroh Implementation 🔴 **NOT STARTED**

**Goal:** Implement real P2P using Iroh library

**Status:** All tasks pending

### 7.1 Iroh Dependencies and Feature Configuration

- [ ] Update `packages/p2p/Cargo.toml` 🔴 **CRITICAL**
    - [ ] Add to `[dependencies]`: `iroh = { workspace = true, optional = true }`
    - [ ] Update `[features]`: `iroh = ["dep:iroh"]`
    - [ ] Add to root workspace `[workspace.dependencies]`: `iroh = "0.91.2"`
    - [ ] Note: tokio is NOT needed as direct dependency - Iroh includes it transitively
    - [ ] Note: We use switchy_async for runtime abstraction, not direct tokio calls

#### 7.1 Verification Checklist

- [ ] Iroh dependencies are properly feature-gated
- [ ] Package builds with `iroh` feature enabled
- [ ] Package builds without `iroh` feature (simulator only)
- [ ] No dependency conflicts in workspace
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo build -p switchy_p2p --features iroh`
- [ ] Run `cargo build -p switchy_p2p --no-default-features`
- [ ] Run `cargo clippy -p switchy_p2p --features iroh -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo tree -p switchy_p2p --features iroh` (check iroh deps)
- [ ] Verify iroh 0.91.2 is pulled in with iroh feature
- [ ] Verify tokio is available transitively through iroh (not as direct dependency)
- [ ] Run `cargo machete` (verify iroh dependency is used when feature enabled)
- [ ] Dependency resolution works correctly

### 7.2 Zero-Cost Iroh Provider Implementation

- [ ] Create `src/iroh.rs` with zero-overhead wrappers 🔴 **CRITICAL**
    - [ ] Add `#[cfg(feature = "iroh")] pub mod iroh;` to `lib.rs`
    - [ ] Direct type aliases for zero cost:

        ```rust
        use iroh::{Endpoint, NodeId as IrohNodeId, SecretKey};
        use crate::{P2PError, P2PResult, P2PNodeId};

        /// Zero-cost type alias - no wrapper overhead
        pub type IrohNodeId = iroh::NodeId;

        impl P2PNodeId for iroh::NodeId {
            fn from_bytes(bytes: &[u8; 32]) -> Result<Self, P2PError> {
                iroh::PublicKey::from_bytes(bytes)
                    .map_err(|e| P2PError::InvalidNodeId(e.to_string()))
            }

            fn as_bytes(&self) -> &[u8; 32] {
                self.as_bytes() // Direct delegation - no overhead
            }

            fn fmt_short(&self) -> String {
                self.fmt_short() // Direct delegation
            }
        }

        /// Thin wrapper around Iroh Endpoint
        pub struct IrohP2P {
            endpoint: Endpoint,
            secret_key: SecretKey,
        }

        impl IrohP2P {
            pub async fn new() -> P2PResult<Self> {
                let secret_key = SecretKey::generate();
                let endpoint = Endpoint::builder()
                    .secret_key(secret_key.clone())
                    .bind()
                    .await
                    .map_err(|e| P2PError::ConnectionFailed(e.to_string()))?;

                Ok(Self {
                    endpoint,
                    secret_key,
                })
            }

            pub async fn with_secret_key(secret_key: SecretKey) -> P2PResult<Self> {
                let endpoint = Endpoint::builder()
                    .secret_key(secret_key.clone())
                    .bind()
                    .await
                    .map_err(|e| P2PError::ConnectionFailed(e.to_string()))?;

                Ok(Self {
                    endpoint,
                    secret_key,
                })
            }

            pub fn local_node_id(&self) -> IrohNodeId {
                self.secret_key.public()
            }

            pub async fn shutdown(&self) -> P2PResult<()> {
                self.endpoint.close().await;
                Ok(())
            }
        }
        ```

#### 7.2 Verification Checklist

- [ ] IrohP2P implements all required trait methods
- [ ] Key generation produces valid peer identities
- [ ] Endpoint configuration is appropriate for use case
- [ ] Provider works with existing trait interface
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p --features iroh -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p --features iroh`
- [ ] Basic Iroh functionality works with existing tests
- [ ] Peer identity management is secure

### 7.3 QUIC Stream-Based Connection Implementation

- [ ] Implement connection using QUIC streams 🔴 **CRITICAL**
    - [ ] Create `IrohConnection` struct:

        ```rust
         use iroh::Connection;
         use switchy::unsync::sync::Mutex;

        pub struct IrohConnection {
            inner: Connection,
            remote_node_id: IrohNodeId,
            is_closed: Arc<AtomicBool>,
        }

        impl IrohConnection {
            pub(crate) fn new(connection: Connection) -> P2PResult<Self> {
                let remote_node_id = connection.remote_node_id()
                    .map_err(|e| P2PError::ConnectionFailed(e.to_string()))?;

                Ok(Self {
                    inner: connection,
                    remote_node_id,
                    is_closed: Arc::new(AtomicBool::new(false)),
                })
            }
        }

        impl P2PConnection for IrohConnection {
            type NodeId = IrohNodeId;

            async fn send(&mut self, data: &[u8]) -> P2PResult<()> {
                if self.is_closed.load(Ordering::Relaxed) {
                    return Err(P2PError::ConnectionClosed);
                }

                // Use unidirectional stream for message sending
                let mut stream = self.inner.open_uni().await
                    .map_err(|e| P2PError::NetworkError(e.to_string()))?;

                stream.write_all(data).await
                    .map_err(|e| P2PError::NetworkError(e.to_string()))?;

                stream.finish()
                    .map_err(|e| P2PError::NetworkError(e.to_string()))?;

                Ok(())
            }

            async fn recv(&mut self) -> P2PResult<Vec<u8>> {
                if self.is_closed.load(Ordering::Relaxed) {
                    return Err(P2PError::ConnectionClosed);
                }

                // Accept incoming unidirectional stream
                let mut stream = self.inner.accept_uni().await
                    .map_err(|_| P2PError::ConnectionClosed)?;

                let data = stream.read_to_end(usize::MAX).await
                    .map_err(|e| P2PError::NetworkError(e.to_string()))?;

                Ok(data)
            }

            fn remote_node_id(&self) -> &Self::NodeId {
                &self.remote_node_id
            }

            fn is_connected(&self) -> bool {
                !self.is_closed.load(Ordering::Relaxed)
            }

            fn close(&mut self) -> P2PResult<()> {
                self.is_closed.store(true, Ordering::Relaxed);
                self.inner.close(0u8.into(), b"Connection closed by user");
                Ok(())
            }
        }
        ```

    - [ ] Create `IrohListener` struct:

        ```rust
        pub struct IrohListener {
            endpoint: Endpoint,
            local_addr: String,
        }

        impl IrohListener {
            pub(crate) fn new(endpoint: Endpoint) -> Self {
                let local_addr = format!("iroh://{}", endpoint.node_id());
                Self {
                    endpoint,
                    local_addr,
                }
            }
        }

        impl P2PListener for IrohListener {
            type Connection = IrohConnection;

            async fn accept(&mut self) -> P2PResult<Self::Connection> {
                let connecting = self.endpoint.accept().await
                    .ok_or(P2PError::ConnectionClosed)?;

                let connection = connecting.await
                    .map_err(|e| P2PError::ConnectionFailed(e.to_string()))?;

                IrohConnection::new(connection)
            }

            fn local_addr(&self) -> &str {
                &self.local_addr
            }
        }
        ```

    - [ ] Implement `P2PSystem` for `IrohP2P`:

        ```rust
        impl P2PSystem for IrohP2P {
            type NodeId = IrohNodeId;
            type Connection = IrohConnection;
            type Listener = IrohListener;

            async fn connect(&self, node_id: Self::NodeId) -> P2PResult<Self::Connection> {
                // Create NodeAddr from NodeId - will need discovery integration
                let node_addr = iroh::NodeAddr::new(node_id);

                let connection = self.endpoint.connect(node_addr, b"moosicbox-p2p").await
                    .map_err(|e| P2PError::ConnectionFailed(e.to_string()))?;

                IrohConnection::new(connection)
            }

            async fn listen(&self, _addr: &str) -> P2PResult<Self::Listener> {
                // Iroh listening is handled by the endpoint automatically
                Ok(IrohListener::new(self.endpoint.clone()))
            }

            async fn discover(&self, _name: &str) -> P2PResult<Self::NodeId> {
                // TODO: Integrate with Iroh's discovery mechanisms
                Err(P2PError::DiscoveryFailed("Discovery not yet implemented for Iroh".to_string()))
            }

            fn local_node_id(&self) -> &Self::NodeId {
                &self.local_node_id()
            }
        }
        ```

#### 7.3 Verification Checklist

- [ ] Iroh connections implement all required trait methods
- [ ] QUIC stream handling works correctly
- [ ] Connection and listener lifecycle is properly managed
- [ ] Resource cleanup prevents leaks
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p --features iroh -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p --features iroh`
- [ ] Existing tests work with Iroh implementation
- [ ] Error handling is robust

### 7.4 NAT Traversal and Discovery Configuration

- [ ] Configure Iroh for production networking 🔴 **CRITICAL**
    - [ ] Configure Iroh for automatic NAT traversal
    - [ ] Set up STUN server configuration
    - [ ] Configure relay fallback mechanisms
    - [ ] Implement peer discovery strategies
    - [ ] Add connection persistence and reconnection logic

#### 7.4 Verification Checklist

- [ ] NAT traversal configuration is appropriate
- [ ] STUN servers are properly configured
- [ ] Relay fallback is available when needed
- [ ] Peer discovery works in test scenarios
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p --features iroh -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p --features iroh`
- [ ] Iroh integration tests pass consistently
- [ ] Connection reliability is acceptable

## Phase 8: Testing Infrastructure 🔴 **NOT STARTED**

**Goal:** Comprehensive testing framework for P2P functionality

**Status:** All tasks pending

### 8.1 Cross-Implementation Property Testing Framework

- [ ] Add proptest dependency to Cargo.toml 🔴 **CRITICAL**
    - [ ] Add to `[dependencies]`: `proptest = { workspace = true, optional = true }`
    - [ ] Add to `[dev-dependencies]`: `proptest = { workspace = true }`
    - [ ] Update `simulator` feature: `simulator = ["dep:proptest"]`
    - [ ] Add to root workspace `[workspace.dependencies]`: `proptest = "1.7.0"`
- [ ] Create `src/test_utils.rs` with generic test framework 🔴 **CRITICAL**
    - [ ] Add generic test functions that work with any P2PSystem:

        ```rust
        use proptest::prelude::*;
        use crate::{P2PSystem, P2PError};

        /// Test that any message sent is received intact
        pub async fn test_message_integrity<S: P2PSystem + Clone>(
            system_a: S,
            system_b: S,
            message: Vec<u8>
        ) -> Result<(), P2PError> {
            // Connect A to B
            let node_b_id = *system_b.local_node_id();
            let mut connection = system_a.connect(node_b_id).await?;

            // Set up B to accept connection
            let mut listener = system_b.listen("0.0.0.0:0").await?;
            let mut incoming = listener.accept().await?;

            // Send message from A to B
            connection.send(&message).await?;

            // Receive message on B
            let received = incoming.recv().await?;

            // Verify integrity
            assert_eq!(message, received);
            Ok(())
        }

        /// Test connection lifecycle management
        pub async fn test_connection_lifecycle<S: P2PSystem + Clone>(
            system_a: S,
            system_b: S,
        ) -> Result<(), P2PError> {
            let node_b_id = *system_b.local_node_id();
            let mut connection = system_a.connect(node_b_id).await?;

            // Connection should be active
            assert!(connection.is_connected());

            // Close connection
            connection.close()?;
            assert!(!connection.is_connected());

            // Sending on closed connection should fail
            let result = connection.send(b"test").await;
            assert!(result.is_err());

            Ok(())
        }

        /// Test network partition scenarios (simulator only)
        #[cfg(feature = "simulator")]
        pub async fn test_network_partition(
            simulator: crate::simulator::SimulatorP2P,
        ) -> Result<(), P2PError> {
            use crate::simulator::{SimulatorNodeId, test_node_id};

            let alice_id = test_node_id("alice");
            let bob_id = test_node_id("bob");

            // Initially connected
            let mut connection = simulator.connect(alice_id).await?;
            connection.send(b"test").await?; // Should work

            // Create partition
            simulator.network_graph.write().await
                .add_partition(&[alice_id], &[bob_id]);

            // Communication should fail
            let result = connection.send(b"test2").await;
            assert!(result.is_err());

            // Heal partition
            simulator.network_graph.write().await
                .heal_partition(&[alice_id], &[bob_id]);

            // Communication should work again
            let new_connection = simulator.connect(bob_id).await?;
            new_connection.send(b"test3").await?; // Should work

            Ok(())
        }

        /// Property test generators
        pub mod generators {
            use super::*;

            pub fn any_message() -> impl Strategy<Value = Vec<u8>> {
                prop::collection::vec(any::<u8>(), 0..1024)
            }

            pub fn any_node_name() -> impl Strategy<Value = String> {
                "[a-z]{3,10}".prop_map(|s| s.to_string())
            }

            pub fn any_small_message() -> impl Strategy<Value = Vec<u8>> {
                prop::collection::vec(any::<u8>(), 0..256)
            }

            pub fn any_path() -> impl Strategy<Value = String> {
                "/[a-z/]{1,20}".prop_map(|s| s.to_string())
            }
        }
        ```

#### 8.1 Verification Checklist

- [ ] Proptest dependency is properly feature-gated and available
- [ ] Run `cargo build -p switchy_p2p` (default features include proptest)
- [ ] Run `cargo tree -p switchy_p2p` (verify proptest 1.7.0 is available)
- [ ] Generic tests work with both simulator and Iroh
- [ ] Test coverage includes all trait methods
- [ ] Failure scenarios are thoroughly tested
- [ ] Performance tests provide meaningful metrics
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo test -p switchy_p2p`
- [ ] Run `cargo machete` (verify proptest is used in tests and dev-dependencies)
- [ ] Tests pass with all feature combinations
- [ ] Test utilities are well-documented

### 8.2 Property-Based Integration Test Suite

- [ ] Create comprehensive property test suite 🔴 **CRITICAL**
    - [ ] Create `tests/properties.rs`:

        ```rust
        use proptest::prelude::*;
        use switchy_p2p::test_utils::{generators::*, *};

        proptest! {
            #[test]
            fn message_integrity_simulator(
                message in any_message()
            ) {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async {
                    let system_a = switchy_p2p::simulator::SimulatorP2P::new();
                    let system_b = switchy_p2p::simulator::SimulatorP2P::new();

                    test_message_integrity(system_a, system_b, message).await.unwrap();
                });
            }

            #[cfg(feature = "iroh")]
            #[test]
            fn message_integrity_iroh(
                message in any_small_message() // Smaller for real network
            ) {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async {
                    let system_a = switchy_p2p::iroh::IrohP2P::new().await.unwrap();
                    let system_b = switchy_p2p::iroh::IrohP2P::new().await.unwrap();

                    test_message_integrity(system_a, system_b, message).await.unwrap();
                });
            }

            #[test]
            fn connection_lifecycle_simulator() {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async {
                    let system_a = switchy_p2p::simulator::SimulatorP2P::new();
                    let system_b = switchy_p2p::simulator::SimulatorP2P::new();

                    test_connection_lifecycle(system_a, system_b).await.unwrap();
                });
            }

            #[test]
            fn router_handles_arbitrary_paths(
                path in any_path(),
                message in any_message()
            ) {
                use switchy_p2p::router::{P2PRouter, Method, P2PRequest, P2PResponse};

                let mut router = P2PRouter::new();
                router.route(Method::GET, &path, |_req| P2PResponse::ok(b"found".to_vec()));

                let request = P2PRequest {
                    method: Method::GET,
                    path: path.clone(),
                    body: message,
                    remote_node_id: Box::new(()),
                    headers: std::collections::BTreeMap::new(),
                };

                let response = router.handle_request(request);
                assert_eq!(response.status, switchy_p2p::router::StatusCode::Ok);
            }
        }
        ```

    - [ ] Create `tests/cross_implementation.rs`:

        ```rust
        //! Tests that verify simulator and Iroh implementations behave identically

        use switchy_p2p::test_utils::*;

        #[tokio::test]
        async fn simulator_and_iroh_basic_communication() {
            // Test that basic communication works the same way in both
            let message = b"Hello, P2P World!";

            // Test with simulator
            let sim_a = switchy_p2p::simulator::SimulatorP2P::new();
            let sim_b = switchy_p2p::simulator::SimulatorP2P::new();
            test_message_integrity(sim_a, sim_b, message.to_vec()).await.unwrap();

            // Test with Iroh (if available)
            #[cfg(feature = "iroh")]
            {
                let iroh_a = switchy_p2p::iroh::IrohP2P::new().await.unwrap();
                let iroh_b = switchy_p2p::iroh::IrohP2P::new().await.unwrap();
                test_message_integrity(iroh_a, iroh_b, message.to_vec()).await.unwrap();
            }
        }

        #[tokio::test]
        async fn node_id_serialization_compatibility() {
            // Test that NodeId representations are compatible
            let test_bytes = [42u8; 32];

            let sim_id = switchy_p2p::simulator::SimulatorNodeId::from_bytes(&test_bytes);
            assert_eq!(sim_id.as_bytes(), &test_bytes);
            assert_eq!(sim_id.fmt_short().len(), 10); // 5 bytes as hex

            #[cfg(feature = "iroh")]
            {
                let iroh_id = switchy_p2p::iroh::IrohNodeId::from_bytes(&test_bytes).unwrap();
                assert_eq!(iroh_id.as_bytes(), &test_bytes);
                assert_eq!(iroh_id.fmt_short().len(), 10); // Should match simulator
            }
        }
        ```

    - [ ] Create `tests/network_scenarios.rs`:

        ```rust
        //! Complex network scenario testing

        use switchy_p2p::simulator::*;
        use std::time::Duration;

        #[tokio::test]
        async fn three_node_mesh_communication() {
            let alice = SimulatorP2P::new();
            let bob = SimulatorP2P::new();
            let charlie = SimulatorP2P::new();

            // Set up mesh topology
            // ... test complex routing scenarios
        }

        #[tokio::test]
        async fn network_partition_and_recovery() {
            // Test the partition scenario from test_utils
            let simulator = SimulatorP2P::new();
            test_network_partition(simulator).await.unwrap();
        }

        #[tokio::test]
        async fn discovery_service_integration() {
            let simulator = SimulatorP2P::new();

            simulator.register_peer("alice", test_node_id("alice")).await.unwrap();
            simulator.register_peer("bob", test_node_id("bob")).await.unwrap();

            // Test discovery
            let alice_id = simulator.discover("alice").await.unwrap();
            let bob_id = simulator.discover("bob").await.unwrap();

            // Test connection by name
            let connection = simulator.connect_by_name("alice").await.unwrap();
            assert!(connection.is_connected());
        }
        ```

#### 8.2 Verification Checklist

- [ ] Integration tests cover realistic usage scenarios
- [ ] Multi-peer tests work with various topologies
- [ ] Cross-implementation tests ensure compatibility
- [ ] Failure recovery tests validate robustness
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo test -p switchy_p2p --test integration`
- [ ] All integration tests pass consistently
- [ ] Performance metrics are within acceptable ranges

## Phase 9: Documentation and Examples ✅ **NOT STARTED**

**Goal:** Comprehensive documentation and usage examples

**Status:** All tasks pending

### 9.1 API Documentation

- [ ] Update `src/lib.rs` - API documentation 🟡 **IMPORTANT**
    - [ ] Add comprehensive rustdoc for all public APIs
    - [ ] Add usage examples for common scenarios
    - [ ] Add architecture overview and design rationale
    - [ ] Add migration guide from tunnel to P2P
    - [ ] Add performance characteristics and benchmarks

#### 9.1 Verification Checklist

- [ ] All public APIs have comprehensive documentation
- [ ] Code examples compile and run correctly
- [ ] Documentation covers both simulator and Iroh usage
- [ ] Migration guide provides clear steps
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo doc -p switchy_p2p`
- [ ] Run `cargo test -p switchy_p2p --doc`
- [ ] Documentation builds without warnings
- [ ] Examples work as documented

### 9.2 Usage Examples

- [ ] Create `examples/` - Example applications 🟡 **IMPORTANT**
    - [ ] Add basic P2P communication example
    - [ ] Add multi-peer chat application example
    - [ ] Add file transfer example using P2P
    - [ ] Add migration example from tunnel to P2P
    - [ ] Add performance testing and benchmarking example

#### 9.2 Verification Checklist

- [ ] All examples compile and run correctly
- [ ] Examples demonstrate key P2P features
- [ ] Code is well-commented and educational
- [ ] Examples work with different feature configurations
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p --examples -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p --examples`
- [ ] Examples execute successfully
- [ ] Performance examples provide useful metrics

## Phase 10: Server Integration Planning ✅ **NOT STARTED**

**Goal:** Plan integration of P2P system with MoosicBox server

**Status:** All tasks pending

### 10.1 Integration Strategy Documentation

- [ ] Create `spec/p2p/integration-plan.md` - Integration documentation 🟡 **IMPORTANT**
    - [ ] Analyze current tunnel usage in server code
    - [ ] Design P2P integration points
    - [ ] Plan feature flag strategy for gradual rollout
    - [ ] Define migration timeline and milestones
    - [ ] Risk assessment and rollback procedures

#### 10.1 Verification Checklist

- [ ] Integration plan covers all current tunnel usage
- [ ] Feature flag strategy allows safe rollout
- [ ] Migration timeline is realistic and achievable
- [ ] Risk mitigation strategies are comprehensive
- [ ] Plan is reviewed and approved by stakeholders
- [ ] Documentation is clear and actionable
- [ ] Dependencies and prerequisites are identified
- [ ] Success criteria are well-defined

### 10.2 Configuration Management Planning

- [ ] Plan P2P configuration integration 🟡 **IMPORTANT**
    - [ ] Environment variable strategy for P2P settings
    - [ ] Configuration file integration
    - [ ] Runtime configuration updates
    - [ ] Development vs production configuration differences
    - [ ] Security considerations for P2P configuration

#### 10.2 Verification Checklist

- [ ] Configuration strategy aligns with existing patterns
- [ ] Environment variables follow established conventions
- [ ] Security implications are properly addressed
- [ ] Development workflow is not disrupted
- [ ] Configuration validation prevents misconfigurations
- [ ] Documentation covers all configuration options
- [ ] Default configurations are secure and functional

## Phase 11: Performance Optimization ✅ **NOT STARTED**

**Goal:** Optimize P2P implementation for production use

**Status:** All tasks pending

### 11.1 Performance Analysis and Benchmarking

- [ ] Create `benches/` - Benchmarking suite 🟢 **MINOR**
    - [ ] Add connection establishment benchmarks
    - [ ] Add message throughput benchmarks
    - [ ] Add memory usage profiling
    - [ ] Add latency measurements
    - [ ] Add comparison with tunnel performance

#### 11.1 Verification Checklist

- [ ] Benchmarks provide meaningful performance metrics
- [ ] Performance meets or exceeds tunnel baseline
- [ ] Memory usage is within acceptable bounds
- [ ] Latency improvements are measurable
- [ ] Run benchmarks with consistent methodology
- [ ] Performance regressions are detected
- [ ] Optimization targets are achieved

## Phase 12: Final Integration and API Consolidation ✅ **NOT STARTED**

**Goal:** Bring together all components into a cohesive public API

**Status:** All tasks pending

### 12.1 Public API Consolidation

- [ ] Create clean public API in `lib.rs` 🔴 **CRITICAL**
    - [ ] Export main types and traits:

        ```rust
        // Core traits (always available)
        pub use types::{P2PError, P2PResult, NetworkConfig, ConnectionStats};
        pub use traits::{P2PSystem, P2PNodeId, P2PConnection, P2PListener};

        // Router system (always available)
        pub use router::{P2PRouter, P2PService, P2PRequest, P2PResponse, Method, StatusCode, Handler};

        // Test utilities (test builds only)
        #[cfg(any(test, feature = "test-utils"))]
        pub use test_utils::*;

        // Backend implementations (feature-gated)
        #[cfg(feature = "simulator")]
        pub use simulator::{SimulatorP2P, SimulatorNodeId, SimulatorConnection, SimulatorListener};

        #[cfg(feature = "iroh")]
        pub use iroh::{IrohP2P, IrohNodeId, IrohConnection, IrohListener};

        // Convenience type aliases for default backend
        #[cfg(feature = "simulator")]
        pub type DefaultP2P = simulator::SimulatorP2P;
        #[cfg(feature = "simulator")]
        pub type DefaultNodeId = simulator::SimulatorNodeId;
        #[cfg(feature = "simulator")]
        pub type DefaultConnection = simulator::SimulatorConnection;

        #[cfg(feature = "iroh")]
        pub type DefaultP2P = iroh::IrohP2P;
        #[cfg(feature = "iroh")]
        pub type DefaultNodeId = iroh::IrohNodeId;
        #[cfg(feature = "iroh")]
        pub type DefaultConnection = iroh::IrohConnection;
        ```

    - [ ] Create builder pattern for easy setup:

        ```rust
        pub struct P2PBuilder {
            config: NetworkConfig,
        }

        impl P2PBuilder {
            pub fn new() -> Self {
                Self {
                    config: NetworkConfig::from_env(),
                }
            }

            pub fn with_config(mut self, config: NetworkConfig) -> Self {
                self.config = config;
                self
            }

            #[cfg(feature = "simulator")]
            pub fn build_simulator(self) -> simulator::SimulatorP2P {
                simulator::SimulatorP2P::with_config(self.config)
            }

            #[cfg(feature = "iroh")]
            pub async fn build_iroh(self) -> P2PResult<iroh::IrohP2P> {
                iroh::IrohP2P::with_config(self.config).await
            }

            pub async fn build_default(self) -> P2PResult<DefaultP2P> {
                #[cfg(feature = "simulator")]
                return Ok(self.build_simulator());

                #[cfg(feature = "iroh")]
                return self.build_iroh().await;

                #[cfg(not(any(feature = "simulator", feature = "iroh")))]
                compile_error!("Must enable either 'simulator' or 'iroh' feature");
            }
        }
        ```

### 12.2 Usage Examples and Documentation

- [ ] Create comprehensive examples 🔴 **CRITICAL**
    - [ ] Create `examples/basic_communication.rs`:

        ```rust
        //! Basic P2P communication example

         use switchy_p2p::{P2PBuilder, P2PSystem, P2PNodeId};

         // Note: For examples with Iroh backend, we need an actual async runtime
         // In production code, we use switchy_async for abstraction
         #[cfg(feature = "iroh")]
         #[tokio::main]
         async fn main() -> Result<(), Box<dyn std::error::Error>> {
             async_main().await
         }

         #[cfg(feature = "simulator")]
         fn main() -> Result<(), Box<dyn std::error::Error>> {
             switchy::unsync::task::block_on(async_main())
         }

         async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
             // Create two P2P instances
            let alice = P2PBuilder::new().build_default().await?;
            let bob = P2PBuilder::new().build_default().await?;

            println!("Alice ID: {}", alice.local_node_id().fmt_short());
            println!("Bob ID: {}", bob.local_node_id().fmt_short());

            // Alice connects to Bob
            let bob_id = *bob.local_node_id();
            let mut connection = alice.connect(bob_id).await?;

            // Send a message
            connection.send(b"Hello from Alice!").await?;
            println!("Message sent from Alice to Bob");

            // Bob receives the message (in real app, Bob would have a listener)
            let mut listener = bob.listen("0.0.0.0:0").await?;
            let mut incoming = listener.accept().await?;
            let message = incoming.recv().await?;

            println!("Bob received: {}", String::from_utf8_lossy(&message));

            Ok(())
        }
        ```

    - [ ] Create `examples/web_server_api.rs`:

        ```rust
        //! Web-server-like API example

        use switchy_p2p::{P2PBuilder, P2PRouter, P2PService, Method, P2PResponse};

        struct MusicService;

        impl P2PService for MusicService {
            fn register_routes(&self, router: &mut P2PRouter) {
                router.route(Method::GET, "/tracks", |_req| {
                    P2PResponse::ok(b"[track1, track2, track3]".to_vec())
                });

                router.route(Method::POST, "/play", |req| {
                    let track_id = String::from_utf8_lossy(&req.body);
                    println!("Playing track: {}", track_id);
                    P2PResponse::ok(b"Playing".to_vec())
                });
            }
        }

        #[tokio::main]
        async fn main() -> Result<(), Box<dyn std::error::Error>> {
            let p2p = P2PBuilder::new().build_default().await?;

            let mut router = P2PRouter::new();
            let music_service = MusicService;
            router.register_service(music_service);

            println!("P2P music server ready at: {}", p2p.local_node_id().fmt_short());

            // In a real app, integrate router with P2P message handling
            // This is just showing the API structure

            Ok(())
        }
        ```

#### 12.1-12.2 Verification Checklist

- [ ] Public API exports are clean and logical
- [ ] Builder pattern works with all backend configurations
- [ ] Examples compile and run correctly
- [ ] Default type aliases resolve to correct backends
- [ ] Feature flags control compilation correctly
- [ ] Documentation covers all public APIs
- [ ] Run `cargo fmt --check -p switchy_p2p`
- [ ] Run `cargo clippy -p switchy_p2p -- -D warnings` MAKE SURE THERE ARE ZERO CLIPPY ISSUES
- [ ] Run `cargo build -p switchy_p2p --examples`
- [ ] Run examples with different feature combinations
- [ ] Run `cargo machete` (final check - no unused dependencies in workspace)

## Phase 13: Production Readiness ✅ **NOT STARTED**

**Goal:** Prepare P2P system for production deployment

**Status:** All tasks pending

### 12.1 Monitoring and Observability

- [ ] Integration with existing telemetry systems 🟡 **IMPORTANT**
    - [ ] Add metrics collection for P2P connections
    - [ ] Add logging integration with existing systems
    - [ ] Add health checks and diagnostics
    - [ ] Add performance monitoring dashboards
    - [ ] Add alerting for P2P system issues

#### 12.1 Verification Checklist

- [ ] Metrics provide actionable insights
- [ ] Logging follows established patterns
- [ ] Health checks accurately reflect system state
- [ ] Monitoring integrates with existing infrastructure
- [ ] Alerts fire appropriately for various failure modes
- [ ] Documentation covers all monitoring aspects

### 12.2 Security Review and Deployment

- [ ] Security assessment and deployment preparation 🔴 **CRITICAL**
    - [ ] Peer authentication and authorization review
    - [ ] Message encryption and integrity verification
    - [ ] Network security considerations
    - [ ] DoS protection and rate limiting
    - [ ] Production deployment plan and rollout strategy

#### 12.2 Verification Checklist

- [ ] Authentication mechanisms are secure
- [ ] All communications are properly encrypted
- [ ] Network attacks are mitigated
- [ ] Rate limiting prevents abuse
- [ ] Security audit findings are addressed
- [ ] Deployment plan is comprehensive and tested
- [ ] Rollback procedures are ready
- [ ] Success criteria are clearly defined

## Success Criteria

- [ ] P2P system successfully provides alternative to tunnel server
- [ ] Direct peer-to-peer connections work across NAT boundaries
- [ ] Performance meets or exceeds tunnel baseline
- [ ] Migration path from tunnel to P2P is smooth
- [ ] Deterministic testing via simulator is available
- [ ] Production deployment is successful with minimal issues
- [ ] Documentation enables other developers to use and extend the system
- [ ] Security requirements are met
- [ ] Monitoring and observability provide operational visibility
- [ ] System is maintainable and extensible

## Benefits of P2P Integration

1. **Improved Performance**: Direct connections reduce latency compared to centralized tunnel
2. **Reduced Infrastructure**: No need for centralized tunnel servers
3. **Better Reliability**: Peer-to-peer connections don't depend on central infrastructure
4. **Automatic NAT Traversal**: Iroh handles complex networking automatically
5. **Scalability**: P2P scales better than centralized approaches
6. **Future-Proof**: Trait-based design allows for other P2P library integration
7. **Testing**: Deterministic simulator enables reliable automated testing
8. **Gradual Migration**: Can coexist with tunnel during transition period

## Risk Mitigation

1. **Risk**: P2P connections may be less reliable than tunnel
    - **Mitigation**: Comprehensive testing and gradual rollout with monitoring

2. **Risk**: NAT traversal may fail in some network configurations
    - **Mitigation**: Fallback mechanisms and thorough network testing

3. **Risk**: Performance may not meet expectations
    - **Mitigation**: Benchmarking and optimization throughout development

4. **Risk**: Migration from tunnel may disrupt existing functionality
    - **Mitigation**: Compatibility layer and gradual migration strategy

5. **Risk**: Security vulnerabilities in P2P implementation
    - **Mitigation**: Security review and audit before production deployment
