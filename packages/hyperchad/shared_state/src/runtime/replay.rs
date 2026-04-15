use hyperchad_shared_state_models::{EventEnvelope, SnapshotEnvelope};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplayBundle {
    pub snapshot: Option<SnapshotEnvelope>,
    pub events: Vec<EventEnvelope>,
}
