#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod in_memory;
#[cfg(feature = "adapter-sse-post-json")]
mod sse_post;
#[cfg(feature = "adapter-ws-json")]
mod websocket;

use async_trait::async_trait;
use flume::Receiver;
use hyperchad_shared_state_models::{
    ChannelId, CommandEnvelope, EventEnvelope, ParticipantId, SnapshotEnvelope, TransportInbound,
    TransportOutbound,
};

pub use in_memory::{InMemoryTransportClient, InMemoryTransportPair};
#[cfg(feature = "adapter-sse-post-json")]
pub use sse_post::SsePostJsonTransportClient;
#[cfg(feature = "adapter-ws-json")]
pub use websocket::WebSocketJsonTransportClient;

/// Supported renderer-neutral shared-state transport families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    WebSocket,
    SsePost,
}

/// Renderer-neutral connection and reconnection behavior.
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

/// Trusted renderer-neutral identity established before shared-state dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedTransportContext {
    pub participant_id: ParticipantId,
    /// Opaque server-owned identity binding used to prevent context swapping.
    pub identity_binding: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelAccess {
    Command,
    Replay,
    Subscribe,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportAuthorizationError {
    #[error("transport access denied: {0}")]
    Denied(String),
    #[error("transport authorization failed: {0}")]
    Operation(String),
}

/// Authorizes shared-state access and projects canonical data for one participant.
#[async_trait]
pub trait SharedStateTransportPolicy: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when the participant may not perform the requested access.
    async fn authorize_channel(
        &self,
        context: &AuthenticatedTransportContext,
        channel_id: &ChannelId,
        access: ChannelAccess,
    ) -> Result<(), TransportAuthorizationError>;

    /// # Errors
    ///
    /// Returns an error when command identity or channel access is unauthorized.
    async fn authorize_command(
        &self,
        context: &AuthenticatedTransportContext,
        command: &CommandEnvelope,
    ) -> Result<(), TransportAuthorizationError> {
        if command.participant_id != context.participant_id {
            return Err(TransportAuthorizationError::Denied(
                "command participant does not match authenticated participant".to_string(),
            ));
        }

        self.authorize_channel(context, &command.channel_id, ChannelAccess::Command)
            .await
    }

    /// Converts a canonical event into an authorized presentation event.
    /// Returning `None` suppresses data not visible to this participant.
    fn project_event(
        &self,
        context: &AuthenticatedTransportContext,
        event: &EventEnvelope,
    ) -> Option<EventEnvelope>;

    /// Converts a canonical snapshot into an authorized presentation snapshot.
    /// Returning `None` suppresses data not visible to this participant.
    fn project_snapshot(
        &self,
        context: &AuthenticatedTransportContext,
        snapshot: &SnapshotEnvelope,
    ) -> Option<SnapshotEnvelope>;
}

pub type SharedStateTransportDispatchError = Box<dyn std::error::Error + Send + Sync>;
pub type SharedStateTransportDispatchResult<T> = Result<T, SharedStateTransportDispatchError>;

/// Renderer-neutral server dispatcher for authenticated shared-state messages.
#[async_trait]
pub trait SharedStateTransportDispatcher: Send + Sync {
    async fn ingest_outbound(
        &self,
        context: &AuthenticatedTransportContext,
        outbound: TransportOutbound,
    ) -> SharedStateTransportDispatchResult<Vec<TransportInbound>>;

    async fn subscribe_channel(
        &self,
        context: &AuthenticatedTransportContext,
        channel_id: &ChannelId,
    ) -> SharedStateTransportDispatchResult<Receiver<EventEnvelope>>;

    fn project_event(
        &self,
        context: &AuthenticatedTransportContext,
        event: &EventEnvelope,
    ) -> Option<EventEnvelope>;
}

#[derive(Debug, Default)]
pub struct AllowAllSharedStateTransportPolicy;

#[async_trait]
impl SharedStateTransportPolicy for AllowAllSharedStateTransportPolicy {
    async fn authorize_channel(
        &self,
        _context: &AuthenticatedTransportContext,
        _channel_id: &ChannelId,
        _access: ChannelAccess,
    ) -> Result<(), TransportAuthorizationError> {
        Ok(())
    }

    fn project_event(
        &self,
        _context: &AuthenticatedTransportContext,
        event: &EventEnvelope,
    ) -> Option<EventEnvelope> {
        Some(event.clone())
    }

    fn project_snapshot(
        &self,
        _context: &AuthenticatedTransportContext,
        snapshot: &SnapshotEnvelope,
    ) -> Option<SnapshotEnvelope> {
        Some(snapshot.clone())
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
