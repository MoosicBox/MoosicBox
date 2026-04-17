use std::collections::BTreeMap;

/// Deterministic manual clock for harness-driven timing.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now_ms: u64,
    scheduled: BTreeMap<u64, Vec<String>>,
}

impl ManualClock {
    /// Creates a new manual clock starting at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            now_ms: 0,
            scheduled: BTreeMap::new(),
        }
    }

    /// Returns current timestamp in milliseconds.
    #[must_use]
    pub const fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Schedules a named task after the given delay.
    pub fn schedule(&mut self, after_ms: u64, name: impl Into<String>) {
        let due = self.now_ms.saturating_add(after_ms);
        self.scheduled.entry(due).or_default().push(name.into());
    }

    /// Advances clock and returns tasks that became due.
    #[must_use]
    pub fn advance(&mut self, by_ms: u64) -> Vec<String> {
        self.now_ms = self.now_ms.saturating_add(by_ms);
        let due_keys = self
            .scheduled
            .keys()
            .copied()
            .filter(|k| *k <= self.now_ms)
            .collect::<Vec<_>>();

        let mut fired = vec![];
        for key in due_keys {
            if let Some(mut names) = self.scheduled.remove(&key) {
                fired.append(&mut names);
            }
        }
        fired
    }
}
