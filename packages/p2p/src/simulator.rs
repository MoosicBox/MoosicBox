//! P2P Network Simulator
//!
//! This module provides a complete P2P network simulation with realistic network conditions,
//! including latency, packet loss, and network partitions. It supports:
//!
//! - Graph-based network topology with configurable links
//! - Realistic network simulation with latency and packet loss
//! - Connection management with async message passing
//! - Network partitions for testing distributed system behavior
//! - Environment-configurable parameters for testing different conditions

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use switchy_async::sync::RwLock;
use switchy_random::{Rng, rng};

/// Get default latency from environment or use 50ms
fn default_latency() -> Duration {
    std::env::var("SIMULATOR_DEFAULT_LATENCY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(Duration::from_millis(50), Duration::from_millis)
}

/// Get default packet loss from environment or use 1%
fn default_packet_loss() -> f64 {
    std::env::var("SIMULATOR_DEFAULT_PACKET_LOSS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.01) // 1% default
}

/// Get discovery delay from environment or use 100ms
#[allow(dead_code)] // Used in Phase 2.4
fn discovery_delay() -> Duration {
    std::env::var("SIMULATOR_DISCOVERY_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(Duration::from_millis(100), Duration::from_millis)
}

/// Get connection timeout from environment or use 30s
#[allow(dead_code)] // Used in Phase 2.3
fn connection_timeout() -> Duration {
    std::env::var("SIMULATOR_CONNECTION_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(Duration::from_secs(30), Duration::from_secs)
}

/// Get max message size from environment or use 1MB
fn max_message_size() -> usize {
    std::env::var("SIMULATOR_MAX_MESSAGE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024 * 1024) // 1MB default
}

/// A network topology graph representing nodes and links in the P2P simulation
///
/// The `NetworkGraph` maintains the complete network topology including all nodes,
/// their connections, and link characteristics like latency and packet loss.
/// It supports dynamic topology changes including network partitions.
#[derive(Debug, Clone)]
pub struct NetworkGraph {
    nodes: BTreeMap<SimulatorNodeId, NodeInfo>,
    links: BTreeMap<(SimulatorNodeId, SimulatorNodeId), LinkInfo>,
}

/// Information about a node in the P2P network
///
/// Contains node identity, online status, registered names for discovery,
/// and message queues for each connected peer to maintain FIFO ordering.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    #[allow(dead_code)] // Used in Phase 2.4
    id: SimulatorNodeId,
    #[allow(dead_code)] // Used in Phase 2.3
    is_online: bool,
    #[allow(dead_code)] // Used in Phase 2.4
    registered_names: BTreeMap<String, String>, // For DNS-like discovery
    message_queues: BTreeMap<SimulatorNodeId, VecDeque<Vec<u8>>>,
}

/// Network link characteristics between two nodes
///
/// Defines the properties of a network connection including latency, packet loss,
/// bandwidth limitations, and whether the link is currently active.
#[derive(Debug, Clone)]
pub struct LinkInfo {
    latency: Duration,
    packet_loss: f64,
    #[allow(dead_code)] // Used in Phase 2.3
    bandwidth_limit: Option<u64>, // bytes per second
    is_active: bool,
}

impl NetworkGraph {
    /// Create a new empty network graph
    ///
    /// Returns a graph with no nodes or links. Nodes and connections can be added
    /// using [`add_node`](Self::add_node) and [`connect_nodes`](Self::connect_nodes).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            links: BTreeMap::new(),
        }
    }

    /// Add a new node to the network graph
    ///
    /// Creates a new node with default state (online, empty message queues, no registered names).
    /// If the node already exists, this operation has no effect.
    pub fn add_node(&mut self, node_id: SimulatorNodeId) {
        self.nodes.insert(
            node_id.clone(),
            NodeInfo {
                id: node_id,
                is_online: true,
                registered_names: BTreeMap::new(),
                message_queues: BTreeMap::new(),
            },
        );
    }

    /// Create a bidirectional connection between two nodes
    ///
    /// Establishes a network link with the specified characteristics (latency, packet loss, etc.).
    /// The connection is automatically bidirectional with identical properties in both directions.
    pub fn connect_nodes(&mut self, a: SimulatorNodeId, b: SimulatorNodeId, link: LinkInfo) {
        self.links.insert((a.clone(), b.clone()), link.clone());
        self.links.insert((b, a), link); // Bidirectional
    }

    /// Find a path between two nodes using breadth-first search
    ///
    /// Searches the network graph for an active route from the source node to the destination node.
    /// Returns the shortest path if one exists, considering only active links.
    ///
    /// # Returns
    ///
    /// * `Some(Vec<SimulatorNodeId>)` - The path from source to destination, including both endpoints
    /// * `None` - No active path exists between the nodes (network partition)
    #[must_use]
    pub fn find_path(
        &self,
        from: SimulatorNodeId,
        to: SimulatorNodeId,
    ) -> Option<Vec<SimulatorNodeId>> {
        if from == to {
            return Some(vec![from]);
        }

        let mut queue = VecDeque::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut parent: BTreeMap<SimulatorNodeId, SimulatorNodeId> = BTreeMap::new();

        queue.push_back(from.clone());
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            // Check all neighbors
            for ((link_from, link_to), link_info) in &self.links {
                if *link_from == current && link_info.is_active {
                    if *link_to == to {
                        // Found path, reconstruct it
                        let mut path = vec![to, current.clone()];
                        let mut node = current;
                        while let Some(prev) = parent.get(&node) {
                            path.push(prev.clone());
                            node = prev.clone();
                        }
                        path.reverse();
                        return Some(path);
                    }

                    if !visited.contains(link_to) {
                        visited.insert(link_to.clone());
                        parent.insert(link_to.clone(), current.clone());
                        queue.push_back(link_to.clone());
                    }
                }
            }
        }

        None // No path found
    }

    /// Create a network partition between two groups of nodes
    ///
    /// Removes all links between nodes in `group_a` and nodes in `group_b`,
    /// simulating a network partition where the groups cannot communicate.
    pub fn add_partition(&mut self, group_a: &[SimulatorNodeId], group_b: &[SimulatorNodeId]) {
        for a in group_a {
            for b in group_b {
                self.links.remove(&(a.clone(), b.clone()));
                self.links.remove(&(b.clone(), a.clone()));
            }
        }
    }

    /// Restore connectivity between two previously partitioned groups
    ///
    /// Re-establishes links between all nodes in `group_a` and all nodes in `group_b`
    /// using default network characteristics (environment-configurable latency and packet loss).
    pub fn heal_partition(&mut self, group_a: &[SimulatorNodeId], group_b: &[SimulatorNodeId]) {
        let default_link = LinkInfo {
            latency: default_latency(),
            packet_loss: default_packet_loss(),
            bandwidth_limit: None,
            is_active: true,
        };

        for a in group_a {
            for b in group_b {
                self.connect_nodes(a.clone(), b.clone(), default_link.clone());
            }
        }
    }

    /// Get mutable reference to a node by its ID
    ///
    /// Returns `None` if the node does not exist in the graph.
    #[must_use]
    pub fn get_node_mut(&mut self, node_id: &SimulatorNodeId) -> Option<&mut NodeInfo> {
        self.nodes.get_mut(node_id)
    }

    /// Get immutable reference to a node by its ID
    ///
    /// Returns `None` if the node does not exist in the graph.
    #[must_use]
    pub fn get_node(&self, node_id: &SimulatorNodeId) -> Option<&NodeInfo> {
        self.nodes.get(node_id)
    }
}

impl Default for NetworkGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A unique identifier for nodes in the P2P network
///
/// 256-bit (32-byte) identifier that uniquely identifies a peer in the network.
/// Supports deterministic generation from seeds for testing and random generation for production.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulatorNodeId([u8; 32]);

impl SimulatorNodeId {
    /// Create a deterministic node ID from a string seed
    /// Used for testing to create predictable node IDs
    #[must_use]
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
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of this node ID
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Format as short hex string for display (first 5 bytes = 10 hex chars)
    #[must_use]
    pub fn fmt_short(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4]
        )
    }

    /// Generate a random node ID (for production use)
    #[must_use]
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
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Main P2P simulation node
///
/// Represents a single peer in the P2P network with its own node identity,
/// shared network topology view, and active connections to other peers.
/// Supports async connection establishment and message passing.
pub struct SimulatorP2P {
    node_id: SimulatorNodeId,
    network_graph: Arc<RwLock<NetworkGraph>>, // NEW in Phase 2.2
    connections: Arc<RwLock<BTreeMap<SimulatorNodeId, SimulatorConnection>>>, // NEW in Phase 2.3
}

impl SimulatorP2P {
    /// Create a new simulator P2P instance with random node ID
    #[must_use]
    pub fn new() -> Self {
        Self {
            node_id: SimulatorNodeId::generate(),
            network_graph: Arc::new(RwLock::new(NetworkGraph::new())),
            connections: Arc::new(RwLock::new(BTreeMap::new())), // NEW
        }
    }

    /// Create a simulator P2P instance with deterministic node ID (for testing)
    #[must_use]
    pub fn with_seed(seed: &str) -> Self {
        Self {
            node_id: SimulatorNodeId::from_seed(seed),
            network_graph: Arc::new(RwLock::new(NetworkGraph::new())),
            connections: Arc::new(RwLock::new(BTreeMap::new())), // NEW
        }
    }

    /// Get this node's ID
    #[must_use]
    pub const fn local_node_id(&self) -> &SimulatorNodeId {
        &self.node_id
    }

    /// Connect to a remote peer
    ///
    /// Establishes a connection to another node in the network, setting up bidirectional
    /// message queues and verifying network connectivity through the topology graph.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * No route exists to the destination node (network partition)
    /// * Connection storage fails
    pub async fn connect(&self, remote_id: SimulatorNodeId) -> Result<SimulatorConnection, String> {
        let mut graph = self.network_graph.write().await;

        // 1. Ensure both nodes exist in graph
        if !graph.nodes.contains_key(&self.node_id) {
            graph.add_node(self.node_id.clone());
        }
        if !graph.nodes.contains_key(&remote_id) {
            graph.add_node(remote_id.clone());
        }

        // 2. Create message queues for bidirectional communication
        if let Some(local_node) = graph.get_node_mut(&self.node_id) {
            local_node
                .message_queues
                .entry(remote_id.clone())
                .or_insert_with(VecDeque::new);
        }
        if let Some(remote_node) = graph.get_node_mut(&remote_id) {
            remote_node
                .message_queues
                .entry(self.node_id.clone())
                .or_insert_with(VecDeque::new);
        }

        // 3. Check connectivity
        let has_path = graph
            .find_path(self.node_id.clone(), remote_id.clone())
            .is_some();
        if !has_path {
            return Err("No route to destination".to_string());
        }

        drop(graph);

        // 4. Create connection
        let connection = SimulatorConnection {
            local_id: self.node_id.clone(),
            remote_id: remote_id.clone(),
            network_graph: Arc::clone(&self.network_graph),
            is_connected: Arc::new(AtomicBool::new(true)),
        };

        // 5. Store in connections map
        {
            let mut connections = self.connections.write().await;
            connections.insert(remote_id, connection.clone());
        }

        Ok(connection)
    }

    /// Register a peer with a discoverable name in the network
    ///
    /// Associates a human-readable name with a node ID, enabling discovery through the
    /// DNS-like lookup system. Names can be used to connect to peers without knowing
    /// their exact node ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the node registration fails
    pub async fn register_peer(&self, name: &str, node_id: SimulatorNodeId) -> Result<(), String> {
        let mut graph = self.network_graph.write().await;

        // Add node to graph if not exists
        if !graph.nodes.contains_key(&node_id) {
            graph.add_node(node_id.clone());
        }

        // Register name in the node's info
        if let Some(node_info) = graph.nodes.get_mut(&node_id) {
            node_info
                .registered_names
                .insert(name.to_string(), node_id.to_string());
        }
        drop(graph);

        Ok(())
    }

    /// Discover a peer by its registered name
    ///
    /// Performs a DNS-like lookup to find the node ID associated with a given name.
    /// Includes simulated network delay to model realistic discovery latency.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The name is not registered with any node
    /// * The discovery service is unavailable
    pub async fn discover(&self, name: &str) -> Result<SimulatorNodeId, String> {
        // Simulate DNS lookup delay
        let delay = discovery_delay();
        switchy_async::time::sleep(delay).await;

        let graph = self.network_graph.read().await;

        // Search through all nodes for registered name
        for (node_id, node_info) in &graph.nodes {
            if node_info.registered_names.contains_key(name) {
                return Ok(node_id.clone());
            }
        }
        drop(graph);

        Err(format!("Name '{name}' not found"))
    }

    /// Connect to a peer by its registered name
    ///
    /// Convenience method that combines discovery and connection establishment.
    /// First discovers the node ID associated with the name, then establishes
    /// a connection to that peer.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * Discovery fails (name not found)
    /// * Connection establishment fails (no route, etc.)
    pub async fn connect_by_name(&self, name: &str) -> Result<SimulatorConnection, String> {
        let node_id = self.discover(name).await?;
        self.connect(node_id).await
    }
}

impl Default for SimulatorP2P {
    fn default() -> Self {
        Self::new()
    }
}

/// An active connection between two peers in the P2P network
///
/// Handles message routing through the network graph with realistic latency
/// and packet loss simulation. Messages are delivered asynchronously with
/// FIFO ordering guarantees.
#[derive(Debug, Clone)]
pub struct SimulatorConnection {
    local_id: SimulatorNodeId,
    remote_id: SimulatorNodeId,
    network_graph: Arc<RwLock<NetworkGraph>>,
    is_connected: Arc<AtomicBool>,
}

impl SimulatorConnection {
    /// Send data to remote peer through network simulation
    ///
    /// Routes the message through the network graph, simulating realistic network conditions
    /// including latency delays and probabilistic packet loss. Messages are delivered
    /// asynchronously with FIFO ordering guarantees.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * Connection is closed
    /// * Message exceeds maximum size limit (configurable via environment)
    /// * No route exists to destination (network partition)
    /// * Network graph access fails
    pub async fn send(&mut self, data: &[u8]) -> Result<(), String> {
        if !self.is_connected.load(Ordering::Relaxed) {
            return Err("Connection closed".to_string());
        }

        // Check message size limit
        let max_size = max_message_size();
        if data.len() > max_size {
            return Err(format!(
                "Message too large: {} bytes exceeds max {}",
                data.len(),
                max_size
            ));
        }

        let graph = self.network_graph.read().await;

        // 1. Find path from local to remote
        let path = graph
            .find_path(self.local_id.clone(), self.remote_id.clone())
            .ok_or_else(|| "No route to destination".to_string())?;

        // 2. Calculate total latency along path
        let total_latency = Self::calculate_path_latency(&graph, &path);

        // 3. Check packet loss along path
        if Self::packet_lost(&graph, &path) {
            return Ok(()); // Packet dropped, but not an error (simulate UDP-like behavior)
        }

        // 4. Sleep for network latency using switchy_async
        drop(graph); // Release lock before sleeping
        switchy_async::time::sleep(total_latency).await;

        // 5. Deliver message to remote's queue
        {
            let mut graph = self.network_graph.write().await;
            if let Some(remote_node) = graph.get_node_mut(&self.remote_id)
                && let Some(queue) = remote_node.message_queues.get_mut(&self.local_id)
            {
                queue.push_back(data.to_vec());
            }
        }

        Ok(())
    }

    /// Receive data from remote peer (non-blocking)
    ///
    /// Attempts to retrieve the next message from this peer's message queue.
    /// Returns immediately if no message is available.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * No message is currently available in the queue
    /// * Network graph access fails
    pub async fn recv(&mut self) -> Result<Vec<u8>, String> {
        {
            let mut graph = self.network_graph.write().await;

            if let Some(local_node) = graph.get_node_mut(&self.local_id)
                && let Some(queue) = local_node.message_queues.get_mut(&self.remote_id)
                && let Some(message) = queue.pop_front()
            {
                return Ok(message);
            }
        }

        Err("No message available".to_string())
    }

    /// Check if connection is still active
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }

    /// Get remote peer's node ID
    #[must_use]
    pub const fn remote_node_id(&self) -> &SimulatorNodeId {
        &self.remote_id
    }

    /// Close the connection
    ///
    /// Marks the connection as disconnected, preventing further message sending.
    /// Existing messages in queues remain available for receiving.
    ///
    /// # Errors
    ///
    /// This method currently always succeeds but returns Result for future extensibility.
    pub fn close(&mut self) -> Result<(), String> {
        self.is_connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Calculate total latency along a path
    fn calculate_path_latency(graph: &NetworkGraph, path: &[SimulatorNodeId]) -> Duration {
        let mut total = Duration::from_millis(0);
        for window in path.windows(2) {
            if let Some(link) = graph.links.get(&(window[0].clone(), window[1].clone())) {
                total += link.latency;
            }
        }
        total
    }

    /// Check if packet should be lost based on path
    fn packet_lost(graph: &NetworkGraph, path: &[SimulatorNodeId]) -> bool {
        for window in path.windows(2) {
            if let Some(link) = graph.links.get(&(window[0].clone(), window[1].clone()))
                && rng().gen_range(0.0..1.0) < link.packet_loss
            {
                return true;
            }
        }
        false
    }
}

impl crate::traits::P2PNodeId for SimulatorNodeId {
    fn from_bytes(bytes: &[u8; 32]) -> crate::types::P2PResult<Self> {
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

/// Create a deterministic node ID for testing
#[must_use]
pub fn test_node_id(name: &str) -> SimulatorNodeId {
    SimulatorNodeId::from_seed(name)
}

#[cfg(test)]
impl SimulatorP2P {
    /// Create a test setup with two connected peers
    ///
    /// Returns a tuple of (`simulator_instance`, `alice_id`, `bob_id`) where Alice and Bob
    /// are connected in the network graph with low-latency, high-reliability links
    /// suitable for testing scenarios.
    #[must_use]
    pub fn test_setup() -> (Self, SimulatorNodeId, SimulatorNodeId) {
        let alice = Self::new();
        let alice_id = alice.local_node_id().clone();

        let bob = Self::new();
        let bob_id = bob.local_node_id().clone();

        // Connect them in the network graph with default link
        {
            let mut graph = alice.network_graph.blocking_write();
            graph.connect_nodes(
                alice_id.clone(),
                bob_id.clone(),
                LinkInfo {
                    latency: Duration::from_millis(10),
                    packet_loss: 0.0,
                    bandwidth_limit: None,
                    is_active: true,
                },
            );
        }

        (alice, alice_id, bob_id)
    }
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
