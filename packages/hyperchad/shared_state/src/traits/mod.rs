use std::collections::BTreeMap;

use async_trait::async_trait;
use flume::Receiver;
use hyperchad_shared_state_models::{
    ChannelId, CommandEnvelope, CommandId, EventEnvelope, IdempotencyKey, PayloadBlob, Revision,
    SnapshotEnvelope,
};

use crate::SharedStateError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginCommandResult {
    New,
    DuplicateApplied {
        command_id: CommandId,
        resulting_revision: Revision,
    },
    DuplicateRejected {
        command_id: CommandId,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDraft {
    pub event_name: String,
    pub payload: PayloadBlob,
    pub metadata: BTreeMap<String, String>,
}

impl EventDraft {
    #[must_use]
    pub fn new(
        event_name: impl Into<String>,
        payload: PayloadBlob,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        Self {
            event_name: event_name.into(),
            payload,
            metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendEventsResult {
    Appended {
        from_revision: Revision,
        to_revision: Revision,
        events: Vec<EventEnvelope>,
    },
    Conflict {
        actual_revision: Revision,
    },
}

#[async_trait]
pub trait CommandStore: Send + Sync {
    async fn begin_command(
        &self,
        command: &CommandEnvelope,
    ) -> Result<BeginCommandResult, SharedStateError>;

    async fn mark_applied(
        &self,
        command_id: &CommandId,
        resulting_revision: Revision,
    ) -> Result<(), SharedStateError>;

    async fn mark_rejected(
        &self,
        command_id: &CommandId,
        reason: &str,
    ) -> Result<(), SharedStateError>;

    async fn load_by_idempotency_key(
        &self,
        channel_id: &ChannelId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<CommandEnvelope>, SharedStateError>;
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append_events(
        &self,
        command: &CommandEnvelope,
        drafts: &[EventDraft],
    ) -> Result<AppendEventsResult, SharedStateError>;

    async fn read_events(
        &self,
        channel_id: &ChannelId,
        from_exclusive_revision: Option<Revision>,
        limit: u32,
    ) -> Result<Vec<EventEnvelope>, SharedStateError>;

    async fn latest_revision(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Option<Revision>, SharedStateError>;
}

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    async fn load_latest_snapshot(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Option<SnapshotEnvelope>, SharedStateError>;

    async fn put_snapshot(&self, snapshot: &SnapshotEnvelope) -> Result<(), SharedStateError>;
}

#[async_trait]
pub trait FanoutBus: Send + Sync {
    async fn publish(&self, event: &EventEnvelope) -> Result<(), SharedStateError>;

    async fn subscribe(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Receiver<EventEnvelope>, SharedStateError>;
}
