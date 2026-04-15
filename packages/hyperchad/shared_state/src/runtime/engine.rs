use std::sync::Arc;

use hyperchad_shared_state_models::{CommandEnvelope, PayloadBlob, SnapshotEnvelope};

use crate::{
    SharedStateError,
    traits::{
        AppendEventsResult, BeginCommandResult, CommandStore, EventDraft, EventStore, FanoutBus,
        SnapshotStore,
    },
};

use super::{ReplayBundle, SnapshotPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyPreparedCommandResult {
    Applied {
        resulting_revision: hyperchad_shared_state_models::Revision,
        emitted_event_count: usize,
    },
    DuplicateApplied {
        command_id: hyperchad_shared_state_models::CommandId,
        resulting_revision: hyperchad_shared_state_models::Revision,
    },
    DuplicateRejected {
        command_id: hyperchad_shared_state_models::CommandId,
        reason: String,
    },
    Conflict {
        actual_revision: hyperchad_shared_state_models::Revision,
    },
}

pub struct SharedStateEngine<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    command_store: Arc<C>,
    event_store: Arc<E>,
    snapshot_store: Arc<S>,
    fanout_bus: Arc<F>,
    snapshot_policy: SnapshotPolicy,
}

impl<C, E, S, F> SharedStateEngine<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    #[must_use]
    pub fn new(
        command_store: Arc<C>,
        event_store: Arc<E>,
        snapshot_store: Arc<S>,
        fanout_bus: Arc<F>,
    ) -> Self {
        Self {
            command_store,
            event_store,
            snapshot_store,
            fanout_bus,
            snapshot_policy: SnapshotPolicy::default(),
        }
    }

    #[must_use]
    pub const fn with_snapshot_policy(mut self, snapshot_policy: SnapshotPolicy) -> Self {
        self.snapshot_policy = snapshot_policy;
        self
    }

    /// # Errors
    ///
    /// * [`SharedStateError::Database`] - If persistence operations fail
    /// * [`SharedStateError::RevisionConflict`] - If expected revision does not match
    pub async fn apply_prepared(
        &self,
        command: &CommandEnvelope,
        drafts: &[EventDraft],
        snapshot_payload: Option<&PayloadBlob>,
    ) -> Result<ApplyPreparedCommandResult, SharedStateError> {
        match self.command_store.begin_command(command).await? {
            BeginCommandResult::DuplicateApplied {
                command_id,
                resulting_revision,
            } => {
                return Ok(ApplyPreparedCommandResult::DuplicateApplied {
                    command_id,
                    resulting_revision,
                });
            }
            BeginCommandResult::DuplicateRejected { command_id, reason } => {
                return Ok(ApplyPreparedCommandResult::DuplicateRejected { command_id, reason });
            }
            BeginCommandResult::New => {}
        }

        let append = self.event_store.append_events(command, drafts).await?;

        match append {
            AppendEventsResult::Conflict { actual_revision } => {
                self.command_store
                    .mark_rejected(
                        &command.command_id,
                        &format!(
                            "Expected revision {} but actual revision is {}",
                            command.expected_revision, actual_revision
                        ),
                    )
                    .await?;

                Ok(ApplyPreparedCommandResult::Conflict { actual_revision })
            }
            AppendEventsResult::Appended {
                from_revision: _,
                to_revision,
                events,
            } => {
                self.command_store
                    .mark_applied(&command.command_id, to_revision)
                    .await?;

                if self.snapshot_policy.should_snapshot(to_revision)
                    && let Some(snapshot_payload) = snapshot_payload
                {
                    let snapshot = SnapshotEnvelope {
                        channel_id: command.channel_id.clone(),
                        revision: to_revision,
                        payload: snapshot_payload.clone(),
                        created_at_ms: command.created_at_ms,
                    };

                    self.snapshot_store.put_snapshot(&snapshot).await?;
                }

                for event in &events {
                    if let Err(error) = self.fanout_bus.publish(event).await {
                        log::warn!(
                            "Failed to publish shared state event {} for channel {}: {error}",
                            event.event_id,
                            event.channel_id
                        );
                    }
                }

                Ok(ApplyPreparedCommandResult::Applied {
                    resulting_revision: to_revision,
                    emitted_event_count: events.len(),
                })
            }
        }
    }

    /// # Errors
    ///
    /// * [`SharedStateError::Database`] - If persistence operations fail
    pub async fn replay_since(
        &self,
        channel_id: &hyperchad_shared_state_models::ChannelId,
        last_seen_revision: Option<hyperchad_shared_state_models::Revision>,
        limit: u32,
    ) -> Result<ReplayBundle, SharedStateError> {
        if let Some(last_seen_revision) = last_seen_revision {
            let events = self
                .event_store
                .read_events(channel_id, Some(last_seen_revision), limit)
                .await?;

            return Ok(ReplayBundle {
                snapshot: None,
                events,
            });
        }

        let snapshot = self.snapshot_store.load_latest_snapshot(channel_id).await?;
        let from_revision = snapshot.as_ref().map(|x| x.revision);
        let events = self
            .event_store
            .read_events(channel_id, from_revision, limit)
            .await?;

        Ok(ReplayBundle { snapshot, events })
    }
}
