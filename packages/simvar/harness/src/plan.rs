//! Interaction planning utilities.
//!
//! This module provides the [`InteractionPlan`] trait for managing sequences of planned
//! interactions in simulations. Use this trait to define deterministic sequences of events
//! or operations that should occur during a simulation run.
//!
//! # Example
//!
//! ```rust
//! use simvar_harness::plan::InteractionPlan;
//!
//! struct MyPlan {
//!     interactions: Vec<String>,
//!     index: usize,
//! }
//!
//! impl InteractionPlan<String> for MyPlan {
//!     fn step(&mut self) -> Option<&String> {
//!         let result = self.interactions.get(self.index);
//!         if result.is_some() {
//!             self.index += 1;
//!         }
//!         result
//!     }
//!
//!     fn gen_interactions(&mut self, count: u64) {
//!         for i in 0..count {
//!             self.interactions.push(format!("interaction-{i}"));
//!         }
//!     }
//!
//!     fn add_interaction(&mut self, interaction: String) {
//!         self.interactions.push(interaction);
//!     }
//! }
//! ```

/// Trait for managing a sequence of planned interactions in a simulation.
///
/// This trait provides a pattern for stepping through a series of interactions,
/// where each call to `step()` returns the next interaction in the sequence.
pub trait InteractionPlan<T>
where
    Self: Sized,
{
    /// Returns the next interaction in the plan, or `None` if exhausted.
    fn step(&mut self) -> Option<&T>;

    /// Generates a specified number of interactions and returns self.
    #[must_use]
    fn with_gen_interactions(mut self, count: u64) -> Self {
        self.gen_interactions(count);
        self
    }

    /// Generates a specified number of interactions.
    fn gen_interactions(&mut self, count: u64);

    /// Adds an interaction to the plan and returns self.
    #[must_use]
    fn with_interaction(mut self, interaction: T) -> Self {
        self.add_interaction(interaction);
        self
    }

    /// Adds an interaction to the plan.
    fn add_interaction(&mut self, interaction: T);
}
