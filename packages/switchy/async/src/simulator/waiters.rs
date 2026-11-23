//! Wakers in one list ordered by when each wait started, for the simulator's own primitives.
//!
//! Every primitive of the simulator keeps its waiters this way (no shards, no random choice and
//! nothing that depends on addresses), so the order it wakes them in, and so the order their
//! tasks are queued on the executor, follows from the order they started waiting.

use std::{collections::BTreeMap, task::Waker};

/// Wakers in the order their waits started.
#[derive(Debug, Default)]
pub struct Waiters {
    next: u64,
    wakers: BTreeMap<u64, Waker>,
}

impl Waiters {
    /// An empty list.
    pub const fn new() -> Self {
        Self {
            next: 0,
            wakers: BTreeMap::new(),
        }
    }

    /// Register (or refresh) the waker of the wait `slot` holds, keeping its place in line if it
    /// is still waiting.
    pub fn register(&mut self, slot: &mut Option<u64>, waker: &Waker) {
        if let Some(entry) = slot.and_then(|id| self.wakers.get_mut(&id)) {
            if !entry.will_wake(waker) {
                entry.clone_from(waker);
            }
            return;
        }
        let id = self.next;
        self.next += 1;
        self.wakers.insert(id, waker.clone());
        *slot = Some(id);
    }

    /// Give up the place of the wait `slot` held, if it is still waiting.
    pub fn remove(&mut self, slot: Option<u64>) {
        if let Some(id) = slot {
            self.wakers.remove(&id);
        }
    }

    /// Every waker, oldest wait first, for waking once the state lock is released.
    pub fn take(&mut self) -> Vec<Waker> {
        std::mem::take(&mut self.wakers).into_values().collect()
    }
}

/// Wake `wakers` in order. Called with no lock of the primitive held.
pub fn wake(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}
