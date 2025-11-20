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
