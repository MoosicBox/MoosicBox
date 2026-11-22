//! [`Checker`]: the bounds and options of a check, and the entry points that run one.

use std::num::NonZeroUsize;

use crate::{Model, ModelReport, bfs, dfs, walk};

/// Random walks a [`Checker`] takes unless told otherwise.
pub const DEFAULT_WALKS: u64 = 100;

/// The most steps a random walk takes unless [`Checker::max_depth`] says otherwise.
pub const DEFAULT_WALK_DEPTH: usize = 100;

/// The bounds and options of a check, and the checkers that run it.
///
/// ```rust
/// # use simvar_explore::{Checker, Model};
/// # struct M;
/// # impl Model for M {
/// #     type State = u8;
/// #     type Action = ();
/// #     fn init_states(&self) -> Vec<u8> { vec![0] }
/// #     fn actions(&self, _: &u8, _: &mut Vec<()>) {}
/// #     fn next_state(&self, _: &u8, _: &()) -> Option<u8> { None }
/// # }
/// # let model = M;
/// // Exhaustive, shortest counterexamples first.
/// let report = Checker::new().max_depth(20).bfs(&model);
/// // Larger configurations: seeded random walks, configurable from the environment.
/// let report = Checker::from_env().walks(1_000).max_depth(200).random_walk(&model);
/// println!("{report}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checker {
    pub(crate) max_depth: Option<usize>,
    pub(crate) max_states: Option<u64>,
    pub(crate) stop_on_failure: bool,
    pub(crate) threads: Option<NonZeroUsize>,
    pub(crate) walks: u64,
    pub(crate) seed: Option<u64>,
    pub(crate) walk_seed: Option<u64>,
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    /// A check with no depth or state limit that stops at the first failure.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_depth: None,
            max_states: None,
            stop_on_failure: true,
            threads: None,
            walks: DEFAULT_WALKS,
            seed: None,
            walk_seed: None,
        }
    }

    /// [`Checker::new`] with the environment's overrides; see [`Checker::env_overrides`].
    ///
    /// # Panics
    ///
    /// * If one of the variables is set but does not parse
    #[must_use]
    pub fn from_env() -> Self {
        Self::new().env_overrides()
    }

    /// Apply the settings in the environment, so a simulation binary's check can be resized
    /// or replayed without rebuilding:
    ///
    /// * `SIMULATOR_EXPLORE_MAX_DEPTH`: [`Checker::max_depth`]
    /// * `SIMULATOR_EXPLORE_MAX_STATES`: [`Checker::max_states`]
    /// * `SIMULATOR_EXPLORE_THREADS`: [`Checker::threads`]
    /// * `SIMULATOR_EXPLORE_WALKS`: [`Checker::walks`]
    /// * `SIMULATOR_EXPLORE_WALK_SEED`: replay the single walk with this seed instead of
    ///   taking new walks (what a failing walk's report prints)
    ///
    /// Random walks also follow `SIMULATOR_SEED` through the simulation run's seed, unless
    /// [`Checker::seed`] fixes theirs.
    ///
    /// # Panics
    ///
    /// * If one of the variables is set but does not parse
    #[must_use]
    pub fn env_overrides(mut self) -> Self {
        if let Some(depth) = env("SIMULATOR_EXPLORE_MAX_DEPTH") {
            self.max_depth = Some(depth);
        }
        if let Some(states) = env("SIMULATOR_EXPLORE_MAX_STATES") {
            self.max_states = Some(states);
        }
        if let Some(threads) = env("SIMULATOR_EXPLORE_THREADS") {
            self.threads = Some(threads);
        }
        if let Some(walks) = env("SIMULATOR_EXPLORE_WALKS") {
            self.walks = walks;
        }
        if let Some(seed) = env("SIMULATOR_EXPLORE_WALK_SEED") {
            self.walk_seed = Some(seed);
        }
        self
    }

    /// Explore at most `depth` actions from an initial state. For random walks this is the
    /// length of each walk (default [`DEFAULT_WALK_DEPTH`]).
    #[must_use]
    pub const fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Visit at most `states` distinct states (searches only).
    #[must_use]
    pub const fn max_states(mut self, states: u64) -> Self {
        self.max_states = Some(states);
        self
    }

    /// Whether to stop at the first counterexample (the default) or keep going until every
    /// property has a discovery or the state space is exhausted.
    #[must_use]
    pub const fn stop_on_failure(mut self, stop: bool) -> Self {
        self.stop_on_failure = stop;
        self
    }

    /// Worker threads for [`Checker::bfs_parallel`] (default: available parallelism).
    #[must_use]
    pub const fn threads(mut self, threads: NonZeroUsize) -> Self {
        self.threads = Some(threads);
        self
    }

    /// Random walks to take (default [`DEFAULT_WALKS`]).
    #[must_use]
    pub const fn walks(mut self, walks: u64) -> Self {
        self.walks = walks;
        self
    }

    /// Draw random walks' seeds from a generator seeded with `seed`, instead of from the
    /// simulation run's stream.
    #[must_use]
    pub const fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Breadth-first search on the current thread. Explores level by level, so every
    /// counterexample and example found is as short as possible.
    pub fn bfs<M: Model>(&self, model: &M) -> ModelReport<M> {
        bfs::sequential(self, model)
    }

    /// Breadth-first search with [`Checker::threads`] worker threads expanding each level.
    /// Finds the same shortest paths and the same counts as [`Checker::bfs`]: successors are
    /// merged in the same order regardless of which thread produced them.
    pub fn bfs_parallel<M>(&self, model: &M) -> ModelReport<M>
    where
        M: Model + Sync,
        M::State: Send + Sync,
        M::Action: Send,
    {
        let threads = self
            .threads
            .or_else(|| std::thread::available_parallelism().ok())
            .map_or(1, NonZeroUsize::get);
        bfs::parallel(self, model, threads)
    }

    /// Depth-first search. Uses memory proportional to the depth for paths (plus the visited
    /// set), but its counterexamples are not the shortest. With [`Checker::max_depth`], a
    /// state is explored again when reached by a shorter path, so nothing within the bound
    /// is missed.
    pub fn dfs<M: Model>(&self, model: &M) -> ModelReport<M> {
        dfs::run(self, model)
    }

    /// Seeded random walks: [`Checker::walks`] walks of at most [`Checker::max_depth`]
    /// steps, each from a uniformly chosen initial state taking uniformly chosen actions.
    ///
    /// Each walk has its own seed, drawn from [`Checker::seed`]'s generator when set and
    /// otherwise from the simulation run's `simvar_explore` stream
    /// ([`switchy_simulation::stream`], so `SIMULATOR_SEED` reproduces a whole run). A
    /// counterexample reports its walk's seed; [`Checker::replay_walk`] (or
    /// `SIMULATOR_EXPLORE_WALK_SEED`) replays exactly that walk.
    pub fn random_walk<M: Model>(&self, model: &M) -> ModelReport<M> {
        walk::run(self, model)
    }

    /// Replay the single random walk with seed `walk_seed`, as reported by a counterexample.
    /// Use the same [`Checker::max_depth`] it was found with.
    pub fn replay_walk<M: Model>(&self, model: &M, walk_seed: u64) -> ModelReport<M> {
        let mut checker = self.clone();
        checker.walk_seed = Some(walk_seed);
        walk::run(&checker, model)
    }
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T>
where
    T::Err: std::fmt::Display,
{
    let value = std::env::var(name).ok()?;
    Some(
        value
            .parse()
            .unwrap_or_else(|e| panic!("{name} must be a number: '{value}' ({e})")),
    )
}
