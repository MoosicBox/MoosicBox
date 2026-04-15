use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ChannelId, CommandId, EventId, IdempotencyKey, ParticipantId, PayloadBlob, Revision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub channel_id: ChannelId,
    pub participant_id: ParticipantId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: Revision,
    pub command_name: String,
    pub payload: PayloadBlob,
    pub metadata: BTreeMap<String, String>,
    pub created_at_ms: i64,
}

impl CommandEnvelope {
    #[must_use]
    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub channel_id: ChannelId,
    pub revision: Revision,
    pub command_id: Option<CommandId>,
    pub event_name: String,
    pub payload: PayloadBlob,
    pub metadata: BTreeMap<String, String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEnvelope {
    pub channel_id: ChannelId,
    pub revision: Revision,
    pub payload: PayloadBlob,
    pub created_at_ms: i64,
}
