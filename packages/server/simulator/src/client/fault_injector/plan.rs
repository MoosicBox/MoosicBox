//! Fault injection interaction plan generation and execution.
//!
//! This module provides the interaction plan system for fault injection testing,
//! which generates random fault injection actions to test system resilience.

use std::time::Duration;

use simvar::{
    plan::InteractionPlan,
    switchy::{
        random::{rand::rand::seq::IteratorRandom as _, rng},
        time::simulator::step_multiplier,
    },
};
use strum::{EnumDiscriminants, EnumIter, IntoEnumIterator as _};

use crate::host::moosicbox_server::HOST;

/// Context for fault injection interaction plan.
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

/// Interaction plan for fault injection testing.
///
/// Generates and executes fault injection interactions including sleeps and bounces.
pub struct FaultInjectionInteractionPlan {
    #[allow(unused)]
    context: InteractionPlanContext,
    step: u64,
    /// The queue of generated interactions to execute.
    pub plan: Vec<Interaction>,
}

impl Default for FaultInjectionInteractionPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultInjectionInteractionPlan {
    /// Creates a new `FaultInjectionInteractionPlan`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            context: InteractionPlanContext::new(),
            step: 0,
            plan: vec![],
        }
    }
}

/// Fault injection interaction type.
#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(InteractionType))]
pub enum Interaction {
    /// Sleep for a duration.
    Sleep(Duration),
    /// Bounce (restart) a host.
    Bounce(String),
}

impl InteractionPlan<Interaction> for FaultInjectionInteractionPlan {
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
        let len = self.plan.len() as u64;

        let mut rng = rng();

        for i in 1..=count {
            loop {
                let interaction_type = InteractionType::iter().choose(&mut rng).unwrap();
                log::trace!(
                    "gen_interactions: generating interaction {i}/{count} ({}) interaction_type={interaction_type:?}",
                    i + len
                );
                match interaction_type {
                    InteractionType::Sleep => {
                        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        self.add_interaction(Interaction::Sleep(Duration::from_millis(
                            rng.gen_range_dist(0..100_000, 0.1) * step_multiplier(),
                        )));
                        break;
                    }
                    InteractionType::Bounce => {
                        if rng.gen_bool(0.99) {
                            continue;
                        }
                        self.add_interaction(Interaction::Bounce(HOST.to_string()));
                        break;
                    }
                }
            }
        }
        drop(rng);
    }

    fn add_interaction(&mut self, interaction: Interaction) {
        log::trace!("add_interaction: adding interaction interaction={interaction:?}");
        match &interaction {
            Interaction::Sleep(..) | Interaction::Bounce(..) => {}
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
    fn test_fault_injection_plan_new() {
        let plan = FaultInjectionInteractionPlan::new();
        assert_eq!(plan.step, 0);
        assert_eq!(plan.plan.len(), 0);
    }

    #[test]
    fn test_fault_injection_plan_default() {
        let plan = FaultInjectionInteractionPlan::default();
        assert_eq!(plan.step, 0);
        assert_eq!(plan.plan.len(), 0);
    }

    #[test]
    fn test_gen_interactions_creates_interactions() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(10);

        // Should create 10 interactions (but doesn't clear, so adds to existing)
        assert_eq!(plan.plan.len(), 10);
    }

    #[test]
    fn test_gen_interactions_contains_sleep_interactions() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(20);

        // Should contain at least some Sleep interactions
        let sleep_count = plan
            .plan
            .iter()
            .filter(|i| matches!(i, Interaction::Sleep(_)))
            .count();
        assert!(sleep_count > 0, "Expected at least one Sleep interaction");
    }

    #[test]
    fn test_gen_interactions_mostly_sleep_rarely_bounce() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(100);

        // Based on the 0.99 probability in gen_bool, bounces should be rare
        let bounce_count = plan
            .plan
            .iter()
            .filter(|i| matches!(i, Interaction::Bounce(_)))
            .count();
        let sleep_count = plan
            .plan
            .iter()
            .filter(|i| matches!(i, Interaction::Sleep(_)))
            .count();

        // Sleep should be much more common than Bounce
        assert!(sleep_count > bounce_count);
    }

    #[test]
    fn test_gen_interactions_bounce_has_correct_host() {
        let mut plan = FaultInjectionInteractionPlan::new();
        // Generate many interactions to increase chance of getting a bounce
        plan.gen_interactions(1000);

        let bounce_interactions: Vec<_> = plan
            .plan
            .iter()
            .filter_map(|i| match i {
                Interaction::Bounce(host) => Some(host),
                Interaction::Sleep(_) => None,
            })
            .collect();

        // Verify any bounces have the correct host
        for host in bounce_interactions {
            assert_eq!(host, HOST);
        }
    }

    #[test]
    fn test_step_returns_interactions_in_order() {
        let mut plan = FaultInjectionInteractionPlan::new();
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
        let mut plan = FaultInjectionInteractionPlan::new();

        assert!(plan.step().is_none());
    }

    #[test]
    fn test_add_interaction_increases_plan_size() {
        let mut plan = FaultInjectionInteractionPlan::new();
        assert_eq!(plan.plan.len(), 0);

        plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));
        assert_eq!(plan.plan.len(), 1);

        plan.add_interaction(Interaction::Bounce(HOST.to_string()));
        assert_eq!(plan.plan.len(), 2);
    }

    #[test]
    fn test_gen_interactions_does_not_clear_previous_plan() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(5);
        let initial_len = plan.plan.len();
        assert_eq!(initial_len, 5);

        plan.gen_interactions(3);
        // Note: Unlike HealthCheckInteractionPlan, this doesn't clear
        assert_eq!(plan.plan.len(), initial_len + 3);
    }

    #[test]
    fn test_gen_interactions_zero_count() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(0);

        assert_eq!(plan.plan.len(), 0);
    }

    #[test]
    fn test_gen_interactions_step_counter_unaffected_by_generation() {
        let mut plan = FaultInjectionInteractionPlan::new();
        plan.gen_interactions(5);

        // Step counter should still be 0 after generation
        assert_eq!(plan.step, 0);

        // Advance the step counter
        let _ = plan.step();
        assert_eq!(plan.step, 1);

        // Generate more interactions
        plan.gen_interactions(3);

        // Step counter should remain unchanged by new generation
        assert_eq!(plan.step, 1);
    }
}
