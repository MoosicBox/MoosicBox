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

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test implementation of `InteractionPlan` for testing purposes
    struct TestPlan {
        interactions: Vec<String>,
        index: usize,
    }

    impl TestPlan {
        fn new() -> Self {
            Self {
                interactions: vec![],
                index: 0,
            }
        }
    }

    impl InteractionPlan<String> for TestPlan {
        fn step(&mut self) -> Option<&String> {
            let result = self.interactions.get(self.index);
            if result.is_some() {
                self.index += 1;
            }
            result
        }

        fn gen_interactions(&mut self, count: u64) {
            for i in 0..count {
                self.interactions.push(format!("generated-{i}"));
            }
        }

        fn add_interaction(&mut self, interaction: String) {
            self.interactions.push(interaction);
        }
    }

    #[test_log::test]
    fn test_step_returns_interactions_in_order() {
        let mut plan = TestPlan::new();
        plan.add_interaction("first".to_string());
        plan.add_interaction("second".to_string());
        plan.add_interaction("third".to_string());

        assert_eq!(plan.step(), Some(&"first".to_string()));
        assert_eq!(plan.step(), Some(&"second".to_string()));
        assert_eq!(plan.step(), Some(&"third".to_string()));
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_step_returns_none_when_exhausted() {
        let mut plan = TestPlan::new();
        plan.add_interaction("only".to_string());

        assert!(plan.step().is_some());
        assert!(plan.step().is_none());
        // Subsequent calls should continue returning None
        assert!(plan.step().is_none());
    }

    #[test_log::test]
    fn test_gen_interactions_generates_specified_count() {
        let mut plan = TestPlan::new();
        plan.gen_interactions(3);

        assert_eq!(plan.interactions.len(), 3);
        assert_eq!(plan.step(), Some(&"generated-0".to_string()));
        assert_eq!(plan.step(), Some(&"generated-1".to_string()));
        assert_eq!(plan.step(), Some(&"generated-2".to_string()));
    }

    #[test_log::test]
    fn test_gen_interactions_with_zero_count() {
        let mut plan = TestPlan::new();
        plan.gen_interactions(0);

        assert!(plan.interactions.is_empty());
        assert!(plan.step().is_none());
    }

    #[test_log::test]
    fn test_with_interaction_returns_self_for_chaining() {
        let plan = TestPlan::new()
            .with_interaction("first".to_string())
            .with_interaction("second".to_string());

        assert_eq!(plan.interactions.len(), 2);
        assert_eq!(plan.interactions[0], "first");
        assert_eq!(plan.interactions[1], "second");
    }

    #[test_log::test]
    fn test_with_gen_interactions_returns_self_for_chaining() {
        let plan = TestPlan::new().with_gen_interactions(3);

        assert_eq!(plan.interactions.len(), 3);
    }

    #[test_log::test]
    fn test_combined_builder_methods() {
        let plan = TestPlan::new()
            .with_interaction("manual-1".to_string())
            .with_gen_interactions(2)
            .with_interaction("manual-2".to_string());

        assert_eq!(plan.interactions.len(), 4);
        assert_eq!(plan.interactions[0], "manual-1");
        assert_eq!(plan.interactions[1], "generated-0");
        assert_eq!(plan.interactions[2], "generated-1");
        assert_eq!(plan.interactions[3], "manual-2");
    }

    #[test_log::test]
    fn test_empty_plan_step_returns_none() {
        let mut plan = TestPlan::new();
        assert!(plan.step().is_none());
    }
}
