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
    fn test_interaction_plan_step_returns_interactions_in_order() {
        let mut plan = TestPlan::new();
        plan.add_interaction("first".to_string());
        plan.add_interaction("second".to_string());
        plan.add_interaction("third".to_string());

        assert_eq!(plan.step(), Some(&"first".to_string()));
        assert_eq!(plan.step(), Some(&"second".to_string()));
        assert_eq!(plan.step(), Some(&"third".to_string()));
    }

    #[test_log::test]
    fn test_interaction_plan_step_returns_none_when_exhausted() {
        let mut plan = TestPlan::new();
        plan.add_interaction("only".to_string());

        assert_eq!(plan.step(), Some(&"only".to_string()));
        assert_eq!(plan.step(), None);
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_interaction_plan_with_interaction_chains_correctly() {
        let mut plan = TestPlan::new()
            .with_interaction("a".to_string())
            .with_interaction("b".to_string());

        assert_eq!(plan.step(), Some(&"a".to_string()));
        assert_eq!(plan.step(), Some(&"b".to_string()));
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_interaction_plan_gen_interactions_creates_correct_count() {
        let mut plan = TestPlan::new();
        plan.gen_interactions(3);

        assert_eq!(plan.interactions.len(), 3);
        assert_eq!(plan.step(), Some(&"generated-0".to_string()));
        assert_eq!(plan.step(), Some(&"generated-1".to_string()));
        assert_eq!(plan.step(), Some(&"generated-2".to_string()));
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_interaction_plan_with_gen_interactions_chains_correctly() {
        let mut plan = TestPlan::new().with_gen_interactions(2);

        assert_eq!(plan.interactions.len(), 2);
        assert_eq!(plan.step(), Some(&"generated-0".to_string()));
        assert_eq!(plan.step(), Some(&"generated-1".to_string()));
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_interaction_plan_mixed_chain_operations() {
        let mut plan = TestPlan::new()
            .with_interaction("manual-1".to_string())
            .with_gen_interactions(2)
            .with_interaction("manual-2".to_string());

        assert_eq!(plan.step(), Some(&"manual-1".to_string()));
        assert_eq!(plan.step(), Some(&"generated-0".to_string()));
        assert_eq!(plan.step(), Some(&"generated-1".to_string()));
        assert_eq!(plan.step(), Some(&"manual-2".to_string()));
        assert_eq!(plan.step(), None);
    }

    #[test_log::test]
    fn test_interaction_plan_gen_interactions_zero_creates_none() {
        let plan = TestPlan::new().with_gen_interactions(0);
        assert!(plan.interactions.is_empty());
    }

    #[test_log::test]
    fn test_interaction_plan_empty_step_returns_none() {
        let mut plan = TestPlan::new();
        assert_eq!(plan.step(), None);
    }
}
