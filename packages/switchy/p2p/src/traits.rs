//! P2P system traits
//!
//! This module defines the core traits that provide abstractions over different
//! P2P implementations. These traits are extracted from the working simulator
//! implementation to ensure they match real functionality.

use crate::types::P2PResult;
use async_trait::async_trait;
use std::fmt::{Debug, Display};

/// P2P system abstraction with async-trait for Send + Sync bounds
/// Using async-trait ensures Future bounds compatibility with async runtimes
///
/// NOTE: `P2PListener` trait and `listen()` method will be added in Phase 5/6
/// when `SimulatorListener` is implemented. Currently excluded because
/// the simulator has no listener functionality yet.
#[async_trait]
pub trait P2PSystem: Send + Sync + 'static {
    /// The type used to uniquely identify nodes in this P2P system
    type NodeId: P2PNodeId;

    /// The type representing an active connection between peers
    type Connection: P2PConnection<NodeId = Self::NodeId>;
    // TODO: Add Listener associated type in Phase 5/6:
    // type Listener: P2PListener<Connection = Self::Connection>;

    /// Connect to a remote peer by node ID
    ///
    /// # Errors
    ///
    /// * Returns an error if the connection fails to establish
    /// * Returns an error if no route exists to the destination node
    async fn connect(&self, node_id: Self::NodeId) -> P2PResult<Self::Connection>;

    /// Discover a peer by name (mock DNS in simulator)
    ///
    /// # Errors
    ///
    /// * Returns an error if the name is not registered with any node
    /// * Returns an error if the discovery service is unavailable
    async fn discover(&self, name: &str) -> P2PResult<Self::NodeId>;

    /// Get this node's ID
    fn local_node_id(&self) -> &Self::NodeId;

    // TODO: Add in Phase 5/6 when SimulatorListener exists:
    // async fn listen(&self, addr: &str) -> P2PResult<Self::Listener>;
}

/// Node identity trait for unique peer identification.
///
/// This trait defines the interface for node identifiers in a P2P network,
/// supporting 256-bit (32-byte) identities compatible with ed25519 public key formats.
/// Implementations must be cloneable, displayable, and thread-safe.
pub trait P2PNodeId: Clone + Debug + Display + Send + Sync + 'static {
    /// Create node ID from 32 bytes (ed25519 public key format)
    ///
    /// # Errors
    ///
    /// Returns an error if the provided bytes cannot be converted to a valid node ID.
    fn from_bytes(bytes: &[u8; 32]) -> P2PResult<Self>;

    /// Get the raw bytes of this node ID
    fn as_bytes(&self) -> &[u8; 32];

    /// Format as short hex string for display
    fn fmt_short(&self) -> String;
}

/// Connection trait for reliable message streams between peers.
///
/// This trait defines the interface for bidirectional communication channels
/// between P2P nodes. Implementations handle message routing, delivery guarantees,
/// and connection lifecycle management.
#[async_trait]
pub trait P2PConnection: Send + Sync + 'static {
    /// The type used to identify the remote peer in this connection
    type NodeId: P2PNodeId;

    /// Send data to remote peer
    ///
    /// # Errors
    ///
    /// * Returns an error if the connection is closed
    /// * Returns an error if the message cannot be delivered
    /// * Returns an error if the message exceeds the maximum size limit
    async fn send(&mut self, data: &[u8]) -> P2PResult<()>;

    /// Receive data from remote peer (non-blocking)
    ///
    /// # Errors
    ///
    /// * Returns an error if no message is currently available
    /// * Returns an error if the connection has failed
    async fn recv(&mut self) -> P2PResult<Vec<u8>>;

    /// Get remote peer's node ID
    fn remote_node_id(&self) -> &Self::NodeId;

    /// Check if connection is still active
    fn is_connected(&self) -> bool;

    /// Close the connection
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be closed properly.
    fn close(&mut self) -> P2PResult<()>;
}

// TODO: P2PListener trait will be added in Phase 5/6 when we implement
// SimulatorListener functionality. Excluded for now as simulator has no
// listener implementation yet.
