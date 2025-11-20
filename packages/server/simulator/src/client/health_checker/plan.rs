//! Health check interaction plan generation and execution.
//!
//! This module provides the interaction plan system for health checking,
//! which generates periodic health check actions to verify server status.

use std::time::Duration;

use simvar::plan::InteractionPlan;
use strum::{EnumDiscriminants, EnumIter};

use crate::host::moosicbox_server::{HOST, PORT};

/// Context for health check interaction plan.
pub struct InteractionPlanContext {}

impl Default for InteractionPlanContext {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionPlanContext {
    /// Creates a new `InteractionPlanContext`.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

/// Interaction plan for health checking.
///
/// Generates and executes health check interactions.
pub struct HealthCheckInteractionPlan {
    #[allow(unused)]
    context: InteractionPlanContext,
    step: u64,
    /// The queue of generated interactions to execute.
    pub plan: Vec<Interaction>,
}

impl Default for HealthCheckInteractionPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheckInteractionPlan {
    /// Creates a new `HealthCheckInteractionPlan`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            context: InteractionPlanContext::new(),
            step: 0,
            plan: vec![],
        }
    }
}

/// Health check interaction type.
#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(InteractionType))]
pub enum Interaction {
    /// Sleep for a duration.
    Sleep(Duration),
    /// Perform a health check on a host.
    HealthCheck(String),
}

impl InteractionPlan<Interaction> for HealthCheckInteractionPlan {
    fn step(&mut self) -> Option<&Interaction> {
        #[allow(clippy::cast_possible_truncation)]
        if let Some(item) = self.plan.get(self.step as usize) {
            self.step += 1;
            log::debug!("step: {}", self.step);
            Some(item)
        } else {
            None
        }
    }

    fn gen_interactions(&mut self, count: u64) {
        self.plan.clear();
        self.step = 0;
        let len = self.plan.len() as u64;

        for i in 1..=count {
            let interaction_type = if (i + len).is_multiple_of(2) {
                InteractionType::Sleep
            } else {
                InteractionType::HealthCheck
            };
            log::trace!(
                "gen_interactions: generating interaction {i}/{count} ({}) interaction_type={interaction_type:?}",
                i + len
            );
            match interaction_type {
                InteractionType::Sleep => {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    self.add_interaction(Interaction::Sleep(Duration::from_millis(1000)));
                }
                InteractionType::HealthCheck => {
                    self.add_interaction(Interaction::HealthCheck(format!("{HOST}:{PORT}")));
                }
            }
        }
    }

    fn add_interaction(&mut self, interaction: Interaction) {
        log::trace!("add_interaction: adding interaction interaction={interaction:?}");
        match &interaction {
            Interaction::Sleep(..) | Interaction::HealthCheck(..) => {}
        }
        self.plan.push(interaction);
    }
}
