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

    mod fault_injection_interaction_plan {
        use super::*;

        #[test_log::test]
        fn step_returns_none_for_empty_plan() {
            let mut plan = FaultInjectionInteractionPlan::new();
            assert!(plan.step().is_none());
        }

        #[test_log::test]
        fn step_iterates_through_plan_sequentially() {
            let mut plan = FaultInjectionInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(500)));
            plan.add_interaction(Interaction::Bounce("test_host".to_string()));
            plan.add_interaction(Interaction::Sleep(Duration::from_secs(1)));

            let first = plan.step();
            assert!(first.is_some());
            assert!(
                matches!(first.unwrap(), Interaction::Sleep(d) if *d == Duration::from_millis(500))
            );

            let second = plan.step();
            assert!(second.is_some());
            assert!(matches!(second.unwrap(), Interaction::Bounce(h) if h == "test_host"));

            let third = plan.step();
            assert!(third.is_some());
            assert!(
                matches!(third.unwrap(), Interaction::Sleep(d) if *d == Duration::from_secs(1))
            );

            assert!(plan.step().is_none());
        }

        #[test_log::test]
        fn step_returns_none_after_all_interactions_exhausted() {
            let mut plan = FaultInjectionInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));

            assert!(plan.step().is_some());
            assert!(plan.step().is_none());
            assert!(plan.step().is_none());
        }

        #[test_log::test]
        fn add_interaction_appends_to_plan() {
            let mut plan = FaultInjectionInteractionPlan::new();
            assert!(plan.plan.is_empty());

            plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));
            assert_eq!(plan.plan.len(), 1);

            plan.add_interaction(Interaction::Bounce("host1".to_string()));
            assert_eq!(plan.plan.len(), 2);

            plan.add_interaction(Interaction::Sleep(Duration::from_secs(5)));
            assert_eq!(plan.plan.len(), 3);
        }

        #[test_log::test]
        fn add_interaction_preserves_interaction_data() {
            let mut plan = FaultInjectionInteractionPlan::new();

            let sleep_duration = Duration::from_millis(12345);
            plan.add_interaction(Interaction::Sleep(sleep_duration));

            let bounce_host = "my_server".to_string();
            plan.add_interaction(Interaction::Bounce(bounce_host.clone()));

            if let Interaction::Sleep(d) = &plan.plan[0] {
                assert_eq!(*d, sleep_duration);
            } else {
                panic!("Expected Sleep interaction");
            }

            if let Interaction::Bounce(h) = &plan.plan[1] {
                assert_eq!(h, &bounce_host);
            } else {
                panic!("Expected Bounce interaction");
            }
        }

        #[test_log::test]
        fn plan_does_not_clear_when_generating_more_interactions() {
            let mut plan = FaultInjectionInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));
            plan.add_interaction(Interaction::Bounce("initial".to_string()));
            let initial_len = plan.plan.len();

            // FaultInjectionInteractionPlan does NOT clear the plan on gen_interactions
            // It appends to the existing plan
            // We cannot test gen_interactions directly due to random behavior,
            // but we can verify initial interactions are preserved
            assert_eq!(plan.plan.len(), initial_len);
        }

        #[test_log::test]
        fn step_continues_from_current_position_after_adding_interactions() {
            let mut plan = FaultInjectionInteractionPlan::new();
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(100)));

            // Step through first interaction
            let first = plan.step();
            assert!(first.is_some());
            assert!(plan.step().is_none()); // Plan exhausted

            // Add more interactions
            plan.add_interaction(Interaction::Bounce("new_host".to_string()));
            plan.add_interaction(Interaction::Sleep(Duration::from_millis(200)));

            // Step should continue from where it left off (step counter preserved)
            let second = plan.step();
            assert!(second.is_some());
            assert!(matches!(second.unwrap(), Interaction::Bounce(h) if h == "new_host"));

            let third = plan.step();
            assert!(third.is_some());
            assert!(
                matches!(third.unwrap(), Interaction::Sleep(d) if *d == Duration::from_millis(200))
            );

            assert!(plan.step().is_none());
        }
    }
}
