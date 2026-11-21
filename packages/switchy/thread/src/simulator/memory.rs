//! Weak memory: which store an atomic load may return.
//!
//! With [`super::Config::weak_memory`], every atomic location keeps a short history of its
//! stores and every simulated thread a vector clock. A load may return any store that the Rust
//! (C++20) memory model allows: not older than what the thread already saw of the location
//! (coherence), and not older than the latest store that happens before it. The scheduler's
//! seeded generator picks among those, so weak behaviour replays like everything else.
//!
//! The model is conservative, so it never invents behaviour real hardware could not produce:
//! `SeqCst` accesses and fences, and every non-atomic synchronization (locks, channels, spawn,
//! join, and any other switch point), act as full fences.

use std::{collections::BTreeMap, sync::atomic::Ordering};

use super::ThreadIndex;

/// The most stores remembered per location; older ones can no longer be read.
const HISTORY: usize = 32;

type Clock = Vec<u64>;

fn join(into: &mut Clock, from: &[u64]) {
    if into.len() < from.len() {
        into.resize(from.len(), 0);
    }
    for (a, b) in into.iter_mut().zip(from) {
        *a = (*a).max(*b);
    }
}

#[derive(Debug, Clone)]
struct Store {
    bits: u64,
    /// The storing thread, or `None` for the value the location was first seen with.
    by: Option<ThreadIndex>,
    /// The storing thread's own clock component after the store.
    stamp: u64,
    /// What a load acquiring this store synchronizes with (release stores and the release
    /// sequences continued by read-modify-writes).
    release: Option<Clock>,
}

#[derive(Debug, Default)]
struct Location {
    stores: Vec<Store>,
    /// Per thread, the earliest store it may still read (coherence).
    seen: BTreeMap<ThreadIndex, usize>,
}

/// The memory model's state for one run.
#[derive(Debug, Default)]
pub struct Memory {
    clocks: Vec<Clock>,
    fence: Clock,
    locations: BTreeMap<usize, Location>,
}

const fn acquires(order: Ordering) -> bool {
    matches!(
        order,
        Ordering::Acquire | Ordering::AcqRel | Ordering::SeqCst
    )
}

const fn releases(order: Ordering) -> bool {
    matches!(
        order,
        Ordering::Release | Ordering::AcqRel | Ordering::SeqCst
    )
}

impl Memory {
    fn clock(&mut self, thread: ThreadIndex) -> &mut Clock {
        if self.clocks.len() <= thread {
            self.clocks.resize(thread + 1, Clock::new());
        }
        let clock = &mut self.clocks[thread];
        if clock.len() <= thread {
            clock.resize(thread + 1, 0);
        }
        clock
    }

    /// A new thread starts with everything its parent has seen.
    pub fn spawned(&mut self, parent: ThreadIndex, child: ThreadIndex) {
        let inherited = self.clock(parent).clone();
        let clock = self.clock(child);
        join(clock, &inherited);
    }

    /// A full fence: the thread sees everything fenced before, and publishes what it saw.
    pub fn fence(&mut self, thread: ThreadIndex) {
        let fence = self.fence.clone();
        let clock = self.clock(thread);
        join(clock, &fence);
        let clock = clock.clone();
        join(&mut self.fence, &clock);
    }

    /// The location at `address`, first seen holding `current`.
    fn location(&mut self, address: usize, current: u64) -> &mut Location {
        self.locations.entry(address).or_insert_with(|| Location {
            stores: vec![Store {
                bits: current,
                by: None,
                stamp: 0,
                release: None,
            }],
            seen: BTreeMap::new(),
        })
    }

    /// What `thread` reads from the atomic at `address` (now holding `current`, its latest
    /// value) with `order`, choosing among allowed stores with `choose(count)`.
    pub fn load(
        &mut self,
        thread: ThreadIndex,
        address: usize,
        current: u64,
        order: Ordering,
        choose: impl FnOnce(usize) -> usize,
    ) -> u64 {
        if order == Ordering::SeqCst {
            self.fence(thread);
        }
        let clock = self.clock(thread).clone();
        let location = self.location(address, current);
        let last = location.stores.len() - 1;
        let happened = location
            .stores
            .iter()
            .rposition(|s| {
                s.by.is_none_or(|by| by == thread || clock.get(by).copied().unwrap_or(0) >= s.stamp)
            })
            .unwrap_or(0);
        let floor = happened.max(location.seen.get(&thread).copied().unwrap_or(0));
        let index = if order == Ordering::SeqCst {
            last
        } else {
            floor + choose(last - floor + 1)
        };
        location.seen.insert(thread, index);
        let store = location.stores[index].clone();
        if acquires(order)
            && let Some(release) = &store.release
        {
            join(self.clock(thread), release);
        }
        store.bits
    }

    fn push(&mut self, thread: ThreadIndex, address: usize, store: Store) {
        let location = self.locations.get_mut(&address).expect("location exists");
        location.stores.push(store);
        if location.stores.len() > HISTORY {
            location.stores.remove(0);
            for seen in location.seen.values_mut() {
                *seen = seen.saturating_sub(1);
            }
        }
        let last = location.stores.len() - 1;
        location.seen.insert(thread, last);
    }

    /// `thread` stores `bits` to the atomic at `address` (holding `current` before) with `order`.
    pub fn store(
        &mut self,
        thread: ThreadIndex,
        address: usize,
        current: u64,
        bits: u64,
        order: Ordering,
    ) {
        let _ = self.location(address, current);
        let clock = self.clock(thread);
        clock[thread] += 1;
        let stamp = clock[thread];
        let release = releases(order).then(|| clock.clone());
        self.push(
            thread,
            address,
            Store {
                bits,
                by: Some(thread),
                stamp,
                release,
            },
        );
        if order == Ordering::SeqCst {
            self.fence(thread);
        }
    }

    /// A read-modify-write by `thread`: it reads the latest store (atomicity), and `update` gives
    /// the value to store, if any. Returns the value read.
    pub fn update(
        &mut self,
        thread: ThreadIndex,
        address: usize,
        current: u64,
        order: Ordering,
        update: impl FnOnce(u64) -> Option<u64>,
    ) -> u64 {
        if order == Ordering::SeqCst {
            self.fence(thread);
        }
        let latest = self
            .location(address, current)
            .stores
            .last()
            .cloned()
            .expect("a location has a store");
        if acquires(order)
            && let Some(release) = &latest.release
        {
            join(self.clock(thread), release);
        }
        if let Some(bits) = update(latest.bits) {
            let clock = self.clock(thread);
            clock[thread] += 1;
            let stamp = clock[thread];
            // A read-modify-write continues the release sequence it read from.
            let mut release = latest.release.clone();
            if releases(order) {
                let own = clock.clone();
                join(release.get_or_insert_with(Clock::new), &own);
            }
            self.push(
                thread,
                address,
                Store {
                    bits,
                    by: Some(thread),
                    stamp,
                    release,
                },
            );
        } else {
            let last = self.locations[&address].stores.len() - 1;
            if let Some(location) = self.locations.get_mut(&address) {
                location.seen.insert(thread, last);
            }
        }
        latest.bits
    }
}
