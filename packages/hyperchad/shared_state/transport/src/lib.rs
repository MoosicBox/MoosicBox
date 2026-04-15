#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod in_memory;

use async_trait::async_trait;
use flume::Receiver;
use hyperchad_shared_state_models::{TransportInbound, TransportOutbound};

pub use in_memory::{InMemoryTransportClient, InMemoryTransportPair};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    WebSocket,
    SsePost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportConfig {
    pub preferred_kind: TransportKind,
    pub heartbeat_interval_ms: u64,
    pub reconnect_initial_backoff_ms: u64,
    pub reconnect_max_backoff_ms: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            preferred_kind: TransportKind::WebSocket,
            heartbeat_interval_ms: 30_000,
            reconnect_initial_backoff_ms: 250,
            reconnect_max_backoff_ms: 10_000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Transport disconnected")]
    Disconnected,
    #[error("Transport operation failed: {0}")]
    Operation(String),
}

#[async_trait]
pub trait SharedStateTransportClient: Send + Sync {
    async fn connect(&self) -> Result<(), TransportError>;
    async fn disconnect(&self) -> Result<(), TransportError>;
    async fn send(&self, message: TransportOutbound) -> Result<(), TransportError>;
    fn inbound(&self) -> Receiver<TransportInbound>;
}
