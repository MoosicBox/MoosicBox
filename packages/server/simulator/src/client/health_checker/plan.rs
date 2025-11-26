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

    mod health_check_interaction_plan {
        use super::*;

        #[test_log::test]
        fn step_returns_none_for_empty_plan() {
            let mut plan = HealthCheckInteractionPlan::new();
            assert!(plan.step().is_none());
        }

        #[test_log::test]
        fn step_iterates_through_plan_sequentially() {
            let mut plan = HealthCheckInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));
            plan.add_interaction(Interaction::HealthCheck("host:1234".to_string()));
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(200)));

            let first = plan.step();
            assert!(first.is_some());
            assert!(
                matches!(first.unwrap(), Interaction::Sleep(d) if *d == Duration::from_millis(100))
            );

            let second = plan.step();
            assert!(second.is_some());
            assert!(matches!(second.unwrap(), Interaction::HealthCheck(h) if h == "host:1234"));

            let third = plan.step();
            assert!(third.is_some());
            assert!(
                matches!(third.unwrap(), Interaction::Sleep(d) if *d == Duration::from_millis(200))
            );

            assert!(plan.step().is_none());
        }

        #[test_log::test]
        fn add_interaction_appends_to_plan() {
            let mut plan = HealthCheckInteractionPlan::new();
            assert!(plan.plan.is_empty());

            plan.add_interaction(Interaction::Sleep(Duration::from_secs(1)));
            assert_eq!(plan.plan.len(), 1);

            plan.add_interaction(Interaction::HealthCheck("localhost:8080".to_string()));
            assert_eq!(plan.plan.len(), 2);
        }

        #[test_log::test]
        fn gen_interactions_creates_alternating_interactions() {
            let mut plan = HealthCheckInteractionPlan::new();
            plan.gen_interactions(4);

            assert_eq!(plan.plan.len(), 4);

            // First interaction (i=1, len=0, total=1): 1 is odd -> HealthCheck
            assert!(matches!(&plan.plan[0], Interaction::HealthCheck(_)));

            // Second interaction (i=2, len=0, total=2): 2 is even -> Sleep
            assert!(matches!(&plan.plan[1], Interaction::Sleep(_)));

            // Third interaction (i=3, len=0, total=3): 3 is odd -> HealthCheck
            assert!(matches!(&plan.plan[2], Interaction::HealthCheck(_)));

            // Fourth interaction (i=4, len=0, total=4): 4 is even -> Sleep
            assert!(matches!(&plan.plan[3], Interaction::Sleep(_)));
        }

        #[test_log::test]
        fn gen_interactions_clears_existing_plan_and_resets_step() {
            let mut plan = HealthCheckInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_secs(1)));
            plan.step(); // advance step to 1

            plan.gen_interactions(2);

            // Should have cleared the old plan and reset step
            assert_eq!(plan.plan.len(), 2);
            // Step should be reset, so we should be able to iterate from the beginning
            let first = plan.step();
            assert!(first.is_some());
        }

        #[test_log::test]
        fn gen_interactions_generates_health_check_with_correct_host_port() {
            let mut plan = HealthCheckInteractionPlan::new();
            plan.gen_interactions(1);

            if let Interaction::HealthCheck(host) = &plan.plan[0] {
                assert_eq!(host, &format!("{HOST}:{PORT}"));
            } else {
                panic!("Expected HealthCheck interaction");
            }
        }

        #[test_log::test]
        fn gen_interactions_generates_sleep_with_1000ms_duration() {
            let mut plan = HealthCheckInteractionPlan::new();
            plan.gen_interactions(2);

            // Second interaction should be Sleep
            if let Interaction::Sleep(duration) = &plan.plan[1] {
                assert_eq!(*duration, Duration::from_millis(1000));
            } else {
                panic!("Expected Sleep interaction");
            }
        }
    }
}
