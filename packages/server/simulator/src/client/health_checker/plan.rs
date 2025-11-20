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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_plan_context_new() {
        let context = InteractionPlanContext::new();
        // Just verify it constructs without panic
        let _ = context;
    }

    #[test]
    fn test_interaction_plan_context_default() {
        let context = InteractionPlanContext::default();
        // Just verify default constructs without panic
        let _ = context;
    }

    #[test]
    fn test_health_check_plan_new() {
        let plan = HealthCheckInteractionPlan::new();
        assert_eq!(plan.step, 0);
        assert_eq!(plan.plan.len(), 0);
    }

    #[test]
    fn test_health_check_plan_default() {
        let plan = HealthCheckInteractionPlan::default();
        assert_eq!(plan.step, 0);
        assert_eq!(plan.plan.len(), 0);
    }

    #[test]
    fn test_gen_interactions_creates_correct_count() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(10);

        assert_eq!(plan.plan.len(), 10);
        assert_eq!(plan.step, 0);
    }

    #[test]
    fn test_gen_interactions_alternates_sleep_and_health_check() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(6);

        // Based on the logic: (i + len).is_multiple_of(2) determines Sleep vs HealthCheck
        // With len=0: i=1 (odd->HealthCheck), i=2 (even->Sleep), i=3 (odd->HealthCheck), etc.
        assert!(matches!(plan.plan[0], Interaction::HealthCheck(_)));
        assert!(matches!(plan.plan[1], Interaction::Sleep(_)));
        assert!(matches!(plan.plan[2], Interaction::HealthCheck(_)));
        assert!(matches!(plan.plan[3], Interaction::Sleep(_)));
        assert!(matches!(plan.plan[4], Interaction::HealthCheck(_)));
        assert!(matches!(plan.plan[5], Interaction::Sleep(_)));
    }

    #[test]
    fn test_gen_interactions_health_check_has_correct_host() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(1);

        if let Interaction::HealthCheck(host) = &plan.plan[0] {
            assert_eq!(host, &format!("{HOST}:{PORT}"));
        } else {
            panic!("Expected HealthCheck interaction");
        }
    }

    #[test]
    fn test_gen_interactions_sleep_has_correct_duration() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(2);

        // Second interaction should be Sleep
        if let Interaction::Sleep(duration) = plan.plan[1] {
            assert_eq!(duration, Duration::from_millis(1000));
        } else {
            panic!("Expected Sleep interaction");
        }
    }

    #[test]
    fn test_step_returns_interactions_in_order() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(3);

        let first = plan.step();
        assert!(first.is_some());
        assert_eq!(plan.step, 1);

        let second = plan.step();
        assert!(second.is_some());
        assert_eq!(plan.step, 2);

        let third = plan.step();
        assert!(third.is_some());
        assert_eq!(plan.step, 3);

        let fourth = plan.step();
        assert!(fourth.is_none());
    }

    #[test]
    fn test_step_returns_none_on_empty_plan() {
        let mut plan = HealthCheckInteractionPlan::new();

        assert!(plan.step().is_none());
    }

    #[test]
    fn test_add_interaction_increases_plan_size() {
        let mut plan = HealthCheckInteractionPlan::new();
        assert_eq!(plan.plan.len(), 0);

        plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));
        assert_eq!(plan.plan.len(), 1);

        plan.add_interaction(Interaction::HealthCheck("test:1234".to_string()));
        assert_eq!(plan.plan.len(), 2);
    }

    #[test]
    fn test_gen_interactions_clears_previous_plan() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(5);
        assert_eq!(plan.plan.len(), 5);

        plan.gen_interactions(3);
        assert_eq!(plan.plan.len(), 3);
        assert_eq!(plan.step, 0);
    }

    #[test]
    fn test_gen_interactions_zero_count() {
        let mut plan = HealthCheckInteractionPlan::new();
        plan.gen_interactions(0);

        assert_eq!(plan.plan.len(), 0);
    }
}
