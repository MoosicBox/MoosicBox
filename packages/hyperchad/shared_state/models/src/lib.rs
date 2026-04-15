#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod envelope;
mod ids;
mod payload;
mod transport;

pub use envelope::{CommandEnvelope, EventEnvelope, SnapshotEnvelope};
pub use ids::{ChannelId, CommandId, EventId, IdempotencyKey, ParticipantId, Revision};
pub use payload::{PayloadBlob, PayloadError, PayloadFormat, PayloadStorage};
pub use transport::{
    TransportInbound, TransportOutbound, TransportPing, TransportSubscribe, TransportUnsubscribe,
};
