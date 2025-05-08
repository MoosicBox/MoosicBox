pub trait InteractionPlan<T>
where
    Self: Sized,
{
    fn step(&mut self) -> Option<&T>;

    #[must_use]
    fn with_gen_interactions(mut self, count: u64) -> Self {
        self.gen_interactions(count);
        self
    }

    fn gen_interactions(&mut self, count: u64);

    #[must_use]
    fn with_interaction(mut self, interaction: T) -> Self {
        self.add_interaction(interaction);
        self
    }

    fn add_interaction(&mut self, interaction: T);
}
