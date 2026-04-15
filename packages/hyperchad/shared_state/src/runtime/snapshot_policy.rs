use hyperchad_shared_state_models::Revision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotPolicy {
    every_n_events: u64,
}

impl SnapshotPolicy {
    #[must_use]
    pub const fn every_n_events(every_n_events: u64) -> Self {
        Self { every_n_events }
    }

    #[must_use]
    pub const fn should_snapshot(self, revision: Revision) -> bool {
        self.every_n_events > 0 && revision.value().is_multiple_of(self.every_n_events)
    }
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self { every_n_events: 25 }
    }
}
