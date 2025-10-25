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
