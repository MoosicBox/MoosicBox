#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, time::SystemTime};

use hyperchad_router::RouteRequest;
use hyperchad_shared_state_models::{
    ChannelId, CommandEnvelope, CommandId, IdempotencyKey, ParticipantId, PayloadBlob, Revision,
};

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Channel resolution failed: {0}")]
    ChannelResolution(String),
    #[error("Participant resolution failed: {0}")]
    ParticipantResolution(String),
    #[error("Invalid system clock: {0}")]
    InvalidClock(String),
}

pub trait SharedStateRouteResolver: Send + Sync {
    /// # Errors
    ///
    /// * [`BridgeError::ChannelResolution`] - If channel resolution fails
    fn resolve_channel_id(&self, request: &RouteRequest) -> Result<ChannelId, BridgeError>;

    /// # Errors
    ///
    /// * [`BridgeError::ParticipantResolution`] - If participant resolution fails
    fn resolve_participant_id(&self, request: &RouteRequest) -> Result<ParticipantId, BridgeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedStateRouteContext {
    pub channel_id: ChannelId,
    pub participant_id: ParticipantId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteCommandInput {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: Revision,
    pub command_name: String,
    pub payload: PayloadBlob,
    pub metadata: BTreeMap<String, String>,
}

impl RouteCommandInput {
    #[must_use]
    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// # Errors
///
/// * [`BridgeError::ChannelResolution`] - If channel ID cannot be resolved
/// * [`BridgeError::ParticipantResolution`] - If participant ID cannot be resolved
pub fn resolve_route_context<R: SharedStateRouteResolver + ?Sized>(
    resolver: &R,
    request: &RouteRequest,
) -> Result<SharedStateRouteContext, BridgeError> {
    Ok(SharedStateRouteContext {
        channel_id: resolver.resolve_channel_id(request)?,
        participant_id: resolver.resolve_participant_id(request)?,
    })
}

/// # Errors
///
/// * [`BridgeError::InvalidClock`] - If current time conversion overflows
pub fn command_from_route(
    context: SharedStateRouteContext,
    input: RouteCommandInput,
) -> Result<CommandEnvelope, BridgeError> {
    let created_at_ms = i64::try_from(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| BridgeError::InvalidClock(e.to_string()))?
            .as_millis(),
    )
    .map_err(|e| BridgeError::InvalidClock(e.to_string()))?;

    Ok(CommandEnvelope {
        command_id: input.command_id,
        channel_id: context.channel_id,
        participant_id: context.participant_id,
        idempotency_key: input.idempotency_key,
        expected_revision: input.expected_revision,
        command_name: input.command_name,
        payload: input.payload,
        metadata: input.metadata,
        created_at_ms,
    })
}
