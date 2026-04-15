use serde::{Deserialize, Serialize};

use crate::{ChannelId, CommandEnvelope, EventEnvelope, Revision, SnapshotEnvelope};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportSubscribe {
    pub channel_id: ChannelId,
    pub last_seen_revision: Option<Revision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportUnsubscribe {
    pub channel_id: ChannelId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportPing {
    pub sent_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportOutbound {
    Command(CommandEnvelope),
    Subscribe(TransportSubscribe),
    Unsubscribe(TransportUnsubscribe),
    Ping(TransportPing),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportInbound {
    Snapshot(SnapshotEnvelope),
    Event(EventEnvelope),
    CommandAccepted {
        command_id: crate::CommandId,
        resulting_revision: Revision,
    },
    CommandRejected {
        command_id: crate::CommandId,
        reason: String,
    },
    Pong(TransportPing),
}
