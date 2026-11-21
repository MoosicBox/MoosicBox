//! Per-run simulation state shared by the switchy simulators.
//!
//! Every switchy simulator (time, random, threads, PTYs, ...) keeps its state for one
//! simulation run in a [`Context`]. Each OS thread has a *current* context; code reaches the
//! run's state through [`current`]. A thread that has never been given a context lazily gets
//! a fresh one of its own, so threads that know nothing about each other stay independent,
//! exactly as thread-local state would.
//!
//! Simulated threads (`switchy_thread`) are real OS threads, so a spawned simulated thread is
//! given its parent's context with [`set_current`] before it runs any user code. All threads of
//! one run then share the same seed, clock and RNG streams. Scheduling guarantees that only one
//! of them runs at a time, so this sharing stays deterministic.
//!
//! The crate also owns the run's seed ([`seed`], read from `SIMULATOR_SEED`) and a portable
//! PRNG ([`Rng`]) with independent named [`stream`]s, so adding draws in one simulator does not
//! perturb another. It has no dependencies beyond `std`.
//!
//! # Examples
//!
//! ```rust
//! use std::sync::Mutex;
//!
//! #[derive(Default)]
//! struct Counter(Mutex<u64>);
//!
//! let counter = switchy_simulation::state::<Counter>();
//! *counter.0.lock().unwrap() += 1;
//! assert_eq!(*switchy_simulation::state::<Counter>().0.lock().unwrap(), 1);
//!
//! let value = switchy_simulation::stream("example").lock().unwrap().next_u64();
//! # let _ = value;
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod rng;

pub use rng::Rng;

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    hash::{BuildHasher as _, Hasher as _},
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

type Slot = Arc<dyn Any + Send + Sync>;

static NEXT_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The state of one simulation run, shared by every thread taking part in it.
///
/// A context holds the run's seed, its named RNG streams, and one value per type for each
/// simulator that keeps per-run state ([`Context::state`]).
pub struct Context {
    id: u64,
    seed: Mutex<u64>,
    streams: Mutex<BTreeMap<String, Arc<Mutex<Rng>>>>,
    slots: Mutex<BTreeMap<TypeId, Slot>>,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("id", &self.id)
            .field("seed", &self.seed())
            .finish_non_exhaustive()
    }
}

impl Context {
    /// A new context with the given seed and no simulator state.
    #[must_use]
    pub fn new(seed: u64) -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
            seed: Mutex::new(seed),
            streams: Mutex::new(BTreeMap::new()),
            slots: Mutex::new(BTreeMap::new()),
        })
    }

    /// A process-unique identifier of this context.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// The run's seed.
    #[must_use]
    pub fn seed(&self) -> u64 {
        *lock(&self.seed)
    }

    /// Replace the run's seed. Every named stream restarts from the new seed.
    pub fn set_seed(&self, seed: u64) {
        *lock(&self.seed) = seed;
        self.reset_streams();
    }

    /// Restart every named stream from the current seed, so each yields its sequence again
    /// from the start. Handles to streams stay valid and observe the restart.
    pub fn reset_streams(&self) {
        let seed = self.seed();
        for (name, stream) in lock(&self.streams).iter() {
            *lock(stream) = Rng::new(stream_seed(seed, name));
        }
    }

    /// The named RNG stream. Each name yields an independent sequence derived from the seed
    /// and the name, so the draws of one simulator never shift another's.
    #[must_use]
    pub fn stream(&self, name: &str) -> Arc<Mutex<Rng>> {
        let mut streams = lock(&self.streams);
        if let Some(stream) = streams.get(name) {
            return Arc::clone(stream);
        }
        let stream = Arc::new(Mutex::new(Rng::new(stream_seed(self.seed(), name))));
        streams.insert(name.to_owned(), Arc::clone(&stream));
        stream
    }

    /// This run's value of `T`, created with `T::default()` on first use.
    #[must_use]
    pub fn state<T: Any + Send + Sync + Default>(&self) -> Arc<T> {
        self.state_with(T::default)
    }

    /// This run's value of `T`, created with `init` on first use.
    ///
    /// # Panics
    ///
    /// * Never in practice: a slot is keyed by its type, so it always holds a `T`.
    #[must_use]
    pub fn state_with<T: Any + Send + Sync>(&self, init: impl FnOnce() -> T) -> Arc<T> {
        let slot = Arc::clone(
            lock(&self.slots)
                .entry(TypeId::of::<T>())
                .or_insert_with(|| Arc::new(init())),
        );
        slot.downcast::<T>()
            .unwrap_or_else(|_| unreachable!("slots are keyed by their type"))
    }

    /// Drop every simulator's state, so each starts fresh on next use. The seed is kept and
    /// named streams restart from it.
    pub fn clear(&self) {
        lock(&self.slots).clear();
        self.reset_streams();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A stable 64-bit FNV-1a hash of `name`, mixed with `seed`, as a stream's seed.
fn stream_seed(seed: u64, name: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    rng::splitmix64(&mut (seed ^ hash))
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Context>>> = const { RefCell::new(None) };
}

/// The current thread's context, creating a fresh one seeded with [`initial_seed`] if the
/// thread has none yet.
#[must_use]
pub fn current() -> Arc<Context> {
    CURRENT.with_borrow_mut(|current| {
        Arc::clone(current.get_or_insert_with(|| Context::new(initial_seed())))
    })
}

/// Make `context` the current thread's context, returning the previous one. `None` leaves the
/// thread without one, so the next [`current`] creates a fresh context.
#[allow(clippy::must_use_candidate)] // installing is the point; the previous value is optional
pub fn set_current(context: Option<Arc<Context>>) -> Option<Arc<Context>> {
    CURRENT.with_borrow_mut(|current| std::mem::replace(current, context))
}

/// Run `f` with `context` as the current thread's context, restoring the previous one
/// afterwards, also when `f` panics.
pub fn with_context<R>(context: Arc<Context>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Arc<Context>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_current(self.0.take());
        }
    }
    let _restore = Restore(set_current(Some(context)));
    f()
}

/// This run's value of `T`; see [`Context::state`].
#[must_use]
pub fn state<T: Any + Send + Sync + Default>() -> Arc<T> {
    current().state::<T>()
}

/// This run's value of `T`, created with `init` on first use; see [`Context::state_with`].
#[must_use]
pub fn state_with<T: Any + Send + Sync>(init: impl FnOnce() -> T) -> Arc<T> {
    current().state_with(init)
}

/// The current run's named RNG stream; see [`Context::stream`].
#[must_use]
pub fn stream(name: &str) -> Arc<Mutex<Rng>> {
    current().stream(name)
}

/// The current run's seed.
#[must_use]
pub fn seed() -> u64 {
    current().seed()
}

/// Set the current run's seed; see [`Context::set_seed`].
pub fn set_seed(seed: u64) {
    current().set_seed(seed);
}

/// Give the current run a new seed from [`gen_seed`].
pub fn reset_seed() {
    set_seed(gen_seed());
}

/// Restart the current run's named streams from its seed; see [`Context::reset_streams`].
pub fn reset_streams() {
    current().reset_streams();
}

static INITIAL_SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED")
        .ok()
        .map_or_else(entropy, |value| {
            value
                .parse::<u64>()
                .unwrap_or_else(|e| panic!("SIMULATOR_SEED must be a u64: '{value}' ({e})"))
        })
});

static SEED_SOURCE: LazyLock<Mutex<Rng>> = LazyLock::new(|| Mutex::new(Rng::new(*INITIAL_SEED)));

/// The process's initial seed: `SIMULATOR_SEED` when set, otherwise random.
///
/// # Panics
///
/// * If `SIMULATOR_SEED` is set but is not a `u64`
#[must_use]
pub fn initial_seed() -> u64 {
    *INITIAL_SEED
}

/// Whether `SIMULATOR_SEED` fixes the initial seed.
#[must_use]
pub fn contains_fixed_seed() -> bool {
    std::env::var("SIMULATOR_SEED").is_ok()
}

/// The next seed in the process-wide sequence derived from [`initial_seed`], for starting
/// another run.
#[must_use]
pub fn gen_seed() -> u64 {
    lock(&SEED_SOURCE).next_u64()
}

/// 64 random bits from the operating system, without dependencies: std seeds its hash keys
/// from the OS random source.
fn entropy() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| {
                u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(0)
            }),
    );
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter(Mutex<u64>);

    #[test]
    fn state_is_per_context_and_shared_within_one() {
        let a = Context::new(1);
        let b = Context::new(1);
        *a.state::<Counter>().0.lock().unwrap() += 2;
        assert_eq!(*a.state::<Counter>().0.lock().unwrap(), 2);
        assert_eq!(*b.state::<Counter>().0.lock().unwrap(), 0);
    }

    #[test]
    fn streams_are_independent_and_reproducible() {
        let a = Context::new(7);
        let b = Context::new(7);
        let x: Vec<u64> = (0..4)
            .map(|_| a.stream("x").lock().unwrap().next_u64())
            .collect();
        let _ = b.stream("y").lock().unwrap().next_u64();
        let x2: Vec<u64> = (0..4)
            .map(|_| b.stream("x").lock().unwrap().next_u64())
            .collect();
        assert_eq!(x, x2, "draws on another stream must not shift this one");
        assert_ne!(x[0], a.stream("y").lock().unwrap().next_u64());
    }

    #[test]
    fn reset_streams_restarts_existing_handles() {
        let context = Context::new(3);
        let handle = context.stream("s");
        let first = handle.lock().unwrap().next_u64();
        let _ = handle.lock().unwrap().next_u64();
        context.reset_streams();
        assert_eq!(handle.lock().unwrap().next_u64(), first);
    }

    #[test]
    fn with_context_installs_and_restores() {
        let outer = current();
        let inner = Context::new(42);
        with_context(Arc::clone(&inner), || {
            assert_eq!(current().id(), inner.id());
            assert_eq!(seed(), 42);
        });
        assert_eq!(current().id(), outer.id());
    }

    #[test]
    fn threads_without_a_context_are_independent() {
        let here = current().id();
        let there = std::thread::spawn(|| current().id()).join().unwrap();
        assert_ne!(here, there);
    }

    #[test]
    fn an_installed_context_is_shared_across_threads() {
        let context = Context::new(9);
        let shared = Arc::clone(&context);
        std::thread::spawn(move || {
            set_current(Some(shared));
            *state::<Counter>().0.lock().unwrap() += 5;
        })
        .join()
        .unwrap();
        assert_eq!(*context.state::<Counter>().0.lock().unwrap(), 5);
    }
}
