use std::collections::hash_map::DefaultHasher;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use switchy_random::{Rng, rng};

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
    // TODO: network_graph will be added in Phase 2.2
    // TODO: connections will be added in Phase 2.3
}

impl SimulatorP2P {
    /// Create a new simulator P2P instance with random node ID
    #[must_use]
    pub fn new() -> Self {
        Self {
            node_id: SimulatorNodeId::generate(),
        }
    }

    /// Create a simulator P2P instance with deterministic node ID (for testing)
    #[must_use]
    pub fn with_seed(seed: &str) -> Self {
        Self {
            node_id: SimulatorNodeId::from_seed(seed),
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
