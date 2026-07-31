use std::sync::Arc;

use crate::{
    runtime::{ApplyPreparedCommandResult, SharedStateEngine},
    traits::{CommandStore, EventDraft, EventStore, FanoutBus, SnapshotStore},
};
use async_trait::async_trait;
use hyperchad_shared_state_models::{
    ChannelId, EventEnvelope, TransportInbound, TransportOutbound,
};
use hyperchad_shared_state_transport::{
    AuthenticatedTransportContext, ChannelAccess, SharedStateTransportDispatchResult,
    SharedStateTransportDispatcher, SharedStateTransportPolicy,
};

/// Runtime dispatcher that applies commands, replays state, and filters delivery by policy.
#[derive(Clone)]
pub struct RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    engine: Arc<SharedStateEngine<C, E, S, F>>,
    fanout_bus: Arc<F>,
    policy: Arc<dyn SharedStateTransportPolicy>,
    replay_limit: u32,
}

impl<C, E, S, F> RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        engine: Arc<SharedStateEngine<C, E, S, F>>,
        fanout_bus: Arc<F>,
        policy: Arc<dyn SharedStateTransportPolicy>,
    ) -> Self {
        Self {
            engine,
            fanout_bus,
            policy,
            replay_limit: 100,
        }
    }

    #[must_use]
    pub const fn with_replay_limit(mut self, replay_limit: u32) -> Self {
        self.replay_limit = replay_limit;
        self
    }
}

#[async_trait]
impl<C, E, S, F> SharedStateTransportDispatcher for RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore + Send + Sync,
    E: EventStore + Send + Sync,
    S: SnapshotStore + Send + Sync,
    F: FanoutBus + Send + Sync,
{
    async fn ingest_outbound(
        &self,
        context: &AuthenticatedTransportContext,
        outbound: TransportOutbound,
    ) -> SharedStateTransportDispatchResult<Vec<TransportInbound>> {
        match outbound {
            TransportOutbound::Command(command) => {
                self.policy.authorize_command(context, &command).await?;
                let drafts = vec![EventDraft::new(
                    command.command_name.clone(),
                    command.payload.clone(),
                    command.metadata.clone(),
                )];
                let result = self.engine.apply_prepared(&command, &drafts, None).await?;
                let inbound = match result {
                    ApplyPreparedCommandResult::Applied {
                        resulting_revision,
                        emitted_event_count: _,
                    }
                    | ApplyPreparedCommandResult::DuplicateApplied {
                        command_id: _,
                        resulting_revision,
                    } => TransportInbound::CommandAccepted {
                        command_id: command.command_id,
                        resulting_revision,
                    },
                    ApplyPreparedCommandResult::DuplicateRejected { command_id, reason } => {
                        TransportInbound::CommandRejected { command_id, reason }
                    }
                    ApplyPreparedCommandResult::Conflict { actual_revision } => {
                        TransportInbound::CommandRejected {
                            command_id: command.command_id,
                            reason: format!(
                                "Expected revision {} but actual revision is {}",
                                command.expected_revision, actual_revision
                            ),
                        }
                    }
                };
                Ok(vec![inbound])
            }
            TransportOutbound::Subscribe(subscribe) => {
                self.policy
                    .authorize_channel(context, &subscribe.channel_id, ChannelAccess::Replay)
                    .await?;
                let replay = self
                    .engine
                    .replay_since(
                        &subscribe.channel_id,
                        subscribe.last_seen_revision,
                        self.replay_limit,
                    )
                    .await?;
                let mut inbound = Vec::new();
                if let Some(snapshot) = replay.snapshot
                    && let Some(snapshot) = self.policy.project_snapshot(context, &snapshot)
                {
                    inbound.push(TransportInbound::Snapshot(snapshot));
                }
                inbound.extend(replay.events.into_iter().filter_map(|event| {
                    self.policy
                        .project_event(context, &event)
                        .map(TransportInbound::Event)
                }));
                Ok(inbound)
            }
            TransportOutbound::Unsubscribe(_) => Ok(Vec::new()),
            TransportOutbound::Ping(ping) => Ok(vec![TransportInbound::Pong(ping)]),
        }
    }

    async fn subscribe_channel(
        &self,
        context: &AuthenticatedTransportContext,
        channel_id: &ChannelId,
    ) -> SharedStateTransportDispatchResult<flume::Receiver<EventEnvelope>> {
        self.policy
            .authorize_channel(context, channel_id, ChannelAccess::Subscribe)
            .await?;
        Ok(self.fanout_bus.subscribe(channel_id).await?)
    }

    fn project_event(
        &self,
        context: &AuthenticatedTransportContext,
        event: &EventEnvelope,
    ) -> Option<EventEnvelope> {
        self.policy.project_event(context, event)
    }
}
