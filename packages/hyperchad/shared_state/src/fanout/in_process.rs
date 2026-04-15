use std::{collections::BTreeMap, sync::RwLock};

use async_trait::async_trait;
use hyperchad_shared_state_models::{ChannelId, EventEnvelope};

use crate::{SharedStateError, traits::FanoutBus};

#[derive(Debug, Default)]
pub struct InProcessFanoutBus {
    subscribers: RwLock<BTreeMap<ChannelId, Vec<flume::Sender<EventEnvelope>>>>,
}

impl InProcessFanoutBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl FanoutBus for InProcessFanoutBus {
    async fn publish(&self, event: &EventEnvelope) -> Result<(), SharedStateError> {
        {
            let mut subscribers = self
                .subscribers
                .write()
                .map_err(|e| SharedStateError::Conversion(format!("Fanout lock poisoned: {e}")))?;

            if let Some(channels) = subscribers.get_mut(&event.channel_id) {
                channels.retain(|sender| sender.send(event.clone()).is_ok());
            }
        }

        Ok(())
    }

    async fn subscribe(
        &self,
        channel_id: &ChannelId,
    ) -> Result<flume::Receiver<EventEnvelope>, SharedStateError> {
        let (sender, receiver) = flume::unbounded();
        {
            let mut subscribers = self
                .subscribers
                .write()
                .map_err(|e| SharedStateError::Conversion(format!("Fanout lock poisoned: {e}")))?;

            subscribers
                .entry(channel_id.clone())
                .or_default()
                .push(sender);
        }

        Ok(receiver)
    }
}
