//! Interleaving coverage: which ordered pairs of conflicting operations a run made.
//!
//! When a run records coverage ([`super::Config::interleaving_coverage`]), the scheduler keeps,
//! for every object an operation touches (a lock, an atomic, a channel, a condition variable, a
//! thread's completion, or another simulator's kernel object), the last thread that wrote it
//! and the threads that read it since, each with the code location ([`Site`]) of its
//! operation. When a thread touches the object and conflicts with one of those (they are
//! different threads and at least one of the two wrote), the pair of their sites, earlier
//! first, is added to the run's [`Coverage`].
//!
//! Sites come from `#[track_caller]` on the public switch points, so they name the caller's
//! code, not this crate's. They are the same in every run of the same build, so coverage can
//! be compared and merged across runs.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    panic::Location,
};

use super::{Object, ThreadIndex};

/// The code location of an operation at a switch point: the caller of a `#[track_caller]`
/// switch point (`file:line:column`).
#[derive(Clone, Copy, Debug)]
pub struct Site(&'static Location<'static>);

impl Site {
    /// The site of the caller.
    #[must_use]
    #[track_caller]
    pub const fn caller() -> Self {
        Self(Location::caller())
    }

    /// The location.
    #[must_use]
    pub const fn location(self) -> &'static Location<'static> {
        self.0
    }

    /// The source file.
    #[must_use]
    pub const fn file(self) -> &'static str {
        self.0.file()
    }

    /// The line, from 1.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.0.line()
    }

    /// The column, from 1.
    #[must_use]
    pub const fn column(self) -> u32 {
        self.0.column()
    }

    const fn key(self) -> (&'static str, u32, u32) {
        (self.file(), self.line(), self.column())
    }

    /// Mix the site into an FNV-1a hash.
    fn hash_into(self, hash: u64) -> u64 {
        let hash = self
            .file()
            .as_bytes()
            .iter()
            .chain(&[0])
            .fold(hash, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(super::FNV_PRIME)
            });
        let hash = super::hash_u64(hash, u64::from(self.line()));
        super::hash_u64(hash, u64::from(self.column()))
    }
}

impl PartialEq for Site {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Site {}

impl PartialOrd for Site {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Site {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

impl std::hash::Hash for Site {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

impl std::fmt::Display for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file(), self.line(), self.column())
    }
}

/// A run's interleaving coverage: ordered pairs of sites of conflicting operations.
///
/// A pair `(a, b)` means one thread operated on an object at `a` and later another thread made
/// a conflicting operation on the same object at `b`. See
/// [`super::Config::interleaving_coverage`].
///
/// Pairs are kept sorted, so everything derived from them is deterministic. Each pair also has
/// a stable 64-bit hash ([`Self::pair_hash`]), the same in every run of the same build, for a
/// fleet to remember which pairs it has seen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pairs: BTreeSet<(Site, Site)>,
}

impl Coverage {
    /// The number of pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether the run recorded no pair.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The pairs, sorted.
    pub fn pairs(&self) -> impl Iterator<Item = (Site, Site)> + '_ {
        self.pairs.iter().copied()
    }

    /// Whether the run recorded the pair `(first, second)`.
    #[must_use]
    pub fn contains(&self, first: Site, second: Site) -> bool {
        self.pairs.contains(&(first, second))
    }

    /// A pair's stable hash: FNV-1a over both sites' file, line and column.
    #[must_use]
    pub fn pair_hash(first: Site, second: Site) -> u64 {
        second.hash_into(first.hash_into(0xcbf2_9ce4_8422_2325))
    }

    /// Every pair's hash.
    #[must_use]
    pub fn hashes(&self) -> BTreeSet<u64> {
        self.pairs().map(|(a, b)| Self::pair_hash(a, b)).collect()
    }

    /// How many of this run's pairs `seen` (a set of [`Self::pair_hash`]es) does not have.
    #[must_use]
    pub fn new_pairs(&self, seen: &BTreeSet<u64>) -> usize {
        self.pairs()
            .filter(|&(a, b)| !seen.contains(&Self::pair_hash(a, b)))
            .count()
    }

    /// Add this run's pairs to `seen`, returning how many were new.
    pub fn merge_into(&self, seen: &mut BTreeSet<u64>) -> usize {
        self.pairs()
            .filter(|&(a, b)| seen.insert(Self::pair_hash(a, b)))
            .count()
    }

    /// A hash of the whole set.
    #[must_use]
    pub fn hash(&self) -> u64 {
        self.pairs().fold(0xcbf2_9ce4_8422_2325, |hash, (a, b)| {
            b.hash_into(a.hash_into(hash))
        })
    }
}

impl std::fmt::Display for Coverage {
    /// One pair per line: `a -> b`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (a, b) in self.pairs() {
            writeln!(f, "{a} -> {b}")?;
        }
        Ok(())
    }
}

/// Who touched an object last.
#[derive(Default)]
struct Accesses {
    /// The last write.
    write: Option<(ThreadIndex, Site)>,
    /// The reads since, each thread and site once.
    reads: Vec<(ThreadIndex, Site)>,
}

/// A run's coverage as it is recorded.
#[derive(Default)]
pub struct Recorder {
    /// Looked up only, never iterated, so the order of ids does not matter.
    objects: BTreeMap<Object, Accesses>,
    coverage: Coverage,
}

impl Recorder {
    /// Thread `thread` touched `object` at `site`, writing it if `write`.
    pub fn access(&mut self, thread: ThreadIndex, object: Object, site: Site, write: bool) {
        // The order of the scheduler's seeded wake-ups is the simulator's, not the program's.
        if object == Object::WakeOrder {
            return;
        }
        let accesses = self.objects.entry(object).or_default();
        let pairs = &mut self.coverage.pairs;
        if let Some((writer, at)) = accesses.write
            && writer != thread
        {
            pairs.insert((at, site));
        }
        if write {
            for (reader, at) in accesses.reads.drain(..) {
                if reader != thread {
                    pairs.insert((at, site));
                }
            }
            accesses.write = Some((thread, site));
        } else if !accesses.reads.contains(&(thread, site)) {
            accesses.reads.push((thread, site));
        }
    }

    pub fn coverage(&self) -> Coverage {
        self.coverage.clone()
    }

    pub fn into_coverage(self) -> Coverage {
        self.coverage
    }
}

thread_local! {
    /// The site of the switch point the calling thread is in, if any.
    static SITE: Cell<Option<Site>> = const { Cell::new(None) };
}

/// The site of the switch point the calling thread is in.
pub fn current() -> Option<Site> {
    SITE.get()
}

/// Restores the previous site on drop; see [`enter`].
pub struct Entered(Option<Site>);

impl Drop for Entered {
    fn drop(&mut self) {
        SITE.set(self.0);
    }
}

/// Make `site` the calling thread's current site until the result is dropped.
pub fn enter_at(site: Site) -> Entered {
    Entered(SITE.replace(Some(site)))
}

/// Make the caller's caller (a `#[track_caller]` switch point's caller) the calling thread's
/// current site until the result is dropped.
#[track_caller]
pub fn enter() -> Entered {
    enter_at(Site::caller())
}
