use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
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
#[allow(dead_code)] // Used in Phase 2.3
fn max_message_size() -> usize {
    std::env::var("SIMULATOR_MAX_MESSAGE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024 * 1024) // 1MB default
}

#[derive(Debug, Clone)]
pub struct NetworkGraph {
    nodes: BTreeMap<SimulatorNodeId, NodeInfo>,
    links: BTreeMap<(SimulatorNodeId, SimulatorNodeId), LinkInfo>,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    #[allow(dead_code)] // Used in Phase 2.4
    id: SimulatorNodeId,
    #[allow(dead_code)] // Used in Phase 2.3
    is_online: bool,
    #[allow(dead_code)] // Used in Phase 2.4
    registered_names: BTreeMap<String, String>, // For DNS-like discovery
    #[allow(dead_code)] // Used in Phase 2.3
    message_queues: BTreeMap<SimulatorNodeId, VecDeque<Vec<u8>>>,
}

#[derive(Debug, Clone)]
pub struct LinkInfo {
    #[allow(dead_code)] // Used in Phase 2.3
    latency: Duration,
    #[allow(dead_code)] // Used in Phase 2.3
    packet_loss: f64,
    #[allow(dead_code)] // Used in Phase 2.3
    bandwidth_limit: Option<u64>, // bytes per second
    is_active: bool,
}

impl NetworkGraph {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            links: BTreeMap::new(),
        }
    }

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

    pub fn connect_nodes(&mut self, a: SimulatorNodeId, b: SimulatorNodeId, link: LinkInfo) {
        self.links.insert((a.clone(), b.clone()), link.clone());
        self.links.insert((b, a), link); // Bidirectional
    }

    /// Find path using simple BFS (for Phase 2, more sophisticated in Phase 6)
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
        let mut visited = std::collections::HashSet::new();
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

    pub fn add_partition(&mut self, group_a: &[SimulatorNodeId], group_b: &[SimulatorNodeId]) {
        for a in group_a {
            for b in group_b {
                self.links.remove(&(a.clone(), b.clone()));
                self.links.remove(&(b.clone(), a.clone()));
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

        for a in group_a {
            for b in group_b {
                self.connect_nodes(a.clone(), b.clone(), default_link.clone());
            }
        }
    }

    #[must_use]
    pub fn get_node_mut(&mut self, node_id: &SimulatorNodeId) -> Option<&mut NodeInfo> {
        self.nodes.get_mut(node_id)
    }

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

pub struct SimulatorP2P {
    node_id: SimulatorNodeId,
    #[allow(dead_code)] // Used in Phase 2.3
    network_graph: Arc<RwLock<NetworkGraph>>, // NEW in Phase 2.2
                                              // TODO: connections will be added in Phase 2.3
}

impl SimulatorP2P {
    /// Create a new simulator P2P instance with random node ID
    #[must_use]
    pub fn new() -> Self {
        Self {
            node_id: SimulatorNodeId::generate(),
            network_graph: Arc::new(RwLock::new(NetworkGraph::new())), // NEW
        }
    }

    /// Create a simulator P2P instance with deterministic node ID (for testing)
    #[must_use]
    pub fn with_seed(seed: &str) -> Self {
        Self {
            node_id: SimulatorNodeId::from_seed(seed),
            network_graph: Arc::new(RwLock::new(NetworkGraph::new())), // NEW
        }
    }

    /// Get this node's ID
    #[must_use]
    pub const fn local_node_id(&self) -> &SimulatorNodeId {
        &self.node_id
    }
}

impl Default for SimulatorP2P {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a deterministic node ID for testing
#[must_use]
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
