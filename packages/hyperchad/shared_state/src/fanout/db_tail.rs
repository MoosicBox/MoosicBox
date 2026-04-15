use std::{collections::BTreeMap, sync::RwLock};

use async_trait::async_trait;
use hyperchad_shared_state_models::{ChannelId, EventEnvelope, Revision};

use crate::{
    SharedStateError,
    traits::{EventStore, FanoutBus},
};

use super::InProcessFanoutBus;

pub struct DbTailFanoutBus<E>
where
    E: EventStore,
{
    event_store: E,
    inner: InProcessFanoutBus,
    cursors: RwLock<BTreeMap<ChannelId, Revision>>,
    poll_limit: u32,
}

impl<E> DbTailFanoutBus<E>
where
    E: EventStore,
{
    #[must_use]
    pub fn new(event_store: E, poll_limit: u32) -> Self {
        Self {
            event_store,
            inner: InProcessFanoutBus::new(),
            cursors: RwLock::new(BTreeMap::new()),
            poll_limit,
        }
    }

    /// # Errors
    ///
    /// * [`SharedStateError::Database`] - If event query fails
    /// * [`SharedStateError::Conversion`] - If fanout cursor lock is poisoned
    pub async fn poll_channel(&self, channel_id: &ChannelId) -> Result<usize, SharedStateError> {
        let from_revision = self
            .cursors
            .read()
            .map_err(|e| SharedStateError::Conversion(format!("Cursor lock poisoned: {e}")))?
            .get(channel_id)
            .copied();

        let events = self
            .event_store
            .read_events(channel_id, from_revision, self.poll_limit)
            .await?;

        if events.is_empty() {
            return Ok(0);
        }

        for event in &events {
            self.inner.publish(event).await?;
        }

        if let Some(last_revision) = events.last().map(|x| x.revision) {
            self.cursors
                .write()
                .map_err(|e| SharedStateError::Conversion(format!("Cursor lock poisoned: {e}")))?
                .insert(channel_id.clone(), last_revision);
        }

        Ok(events.len())
    }
}

#[async_trait]
impl<E> FanoutBus for DbTailFanoutBus<E>
where
    E: EventStore + Send + Sync,
{
    async fn publish(&self, event: &EventEnvelope) -> Result<(), SharedStateError> {
        self.inner.publish(event).await
    }

    async fn subscribe(
        &self,
        channel_id: &ChannelId,
    ) -> Result<flume::Receiver<EventEnvelope>, SharedStateError> {
        self.inner.subscribe(channel_id).await
    }
}
