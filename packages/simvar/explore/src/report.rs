//! What a check found: per-property outcomes, counts and how to reproduce it.

use std::{fmt, time::Duration};

use crate::{Expectation, Model, Trace};

/// The report of checking a [`Model`]; see [`Report`].
pub type ModelReport<M> = Report<<M as Model>::State, <M as Model>::Action>;

/// What a checker found.
///
/// Display prints a summary line, then each property's outcome with its counterexample or
/// example; the alternate form (`{:#}`) prints every state of those paths.
#[derive(Debug, Clone)]
pub struct Report<S, A> {
    /// How the model was explored.
    pub strategy: Strategy,
    /// How much was explored.
    pub stats: Stats,
    /// Whether the whole reachable state space (within the boundary) was explored: no depth
    /// or state limit cut it short and the check did not stop early.
    pub complete: bool,
    /// Each property's outcome, in the order the model listed them.
    pub properties: Vec<PropertyReport<S, A>>,
}

impl<S, A> Report<S, A> {
    /// Whether every property passed: no safety or liveness counterexample was found, and
    /// every `sometimes` property was shown reachable.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.properties.iter().all(|p| p.outcome.is_success())
    }

    /// The properties that failed.
    pub fn failures(&self) -> impl Iterator<Item = &PropertyReport<S, A>> {
        self.properties.iter().filter(|p| !p.outcome.is_success())
    }

    /// The outcome of the property named `name`.
    #[must_use]
    pub fn property(&self, name: &str) -> Option<&PropertyReport<S, A>> {
        self.properties.iter().find(|p| p.name == name)
    }
}

impl<S: fmt::Debug, A: fmt::Debug> Report<S, A> {
    /// Panic with the report unless [`Report::is_success`].
    ///
    /// # Panics
    ///
    /// * If any property failed
    pub fn assert_success(&self) {
        assert!(self.is_success(), "model check failed:\n{self:#}");
    }
}

/// How a model was explored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategy {
    /// Breadth-first search: counterexamples and examples are as short as possible.
    Bfs {
        /// Worker threads expanding states.
        threads: usize,
    },
    /// Depth-first search.
    Dfs,
    /// Seeded random walks from the initial states.
    RandomWalk {
        /// Walks taken.
        walks: u64,
        /// The most actions a walk takes.
        max_depth: usize,
        /// Where the walks' seeds came from.
        seeds: SeedSource,
    },
}

/// Where random walks' seeds came from, so a run can be reproduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedSource {
    /// Drawn from the simulation run's `simvar_explore` stream
    /// ([`switchy_simulation::stream`]), so `SIMULATOR_SEED` set to `run_seed` reproduces
    /// them in a fresh process.
    Run {
        /// The run's seed.
        run_seed: u64,
    },
    /// Drawn from a generator seeded with the seed given to
    /// [`Checker::seed`](crate::Checker::seed).
    Explicit(u64),
    /// A single walk replayed from its own seed.
    Replay(u64),
}

/// How much a checker explored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// States generated, counting a state each time it was reached.
    pub states: u64,
    /// Distinct states visited (after symmetry reduction). With `eventually` properties a
    /// state reached with different properties still pending counts once per combination.
    pub unique: u64,
    /// The most actions from an initial state to a visited state.
    pub max_depth: usize,
    /// Random walks taken (zero for searches).
    pub walks: u64,
    /// Wall-clock time the check took.
    pub duration: Duration,
}

/// One property's outcome.
#[derive(Debug, Clone)]
pub struct PropertyReport<S, A> {
    /// The property's name.
    pub name: String,
    /// What it claims.
    pub expectation: Expectation,
    /// What the checker found.
    pub outcome: Outcome<S, A>,
}

/// What a checker found about one property.
#[derive(Debug, Clone)]
pub enum Outcome<S, A> {
    /// An `always` or `eventually` property was checked on the whole reachable state space
    /// and holds (for `eventually`, on every terminating path).
    Holds,
    /// An `always` or `eventually` property had no counterexample in the part of the state
    /// space explored, which was not all of it.
    Unrefuted,
    /// A counterexample: for `always`, a path to a state where the condition does not hold;
    /// for `eventually`, a path to a terminal state along which it never held.
    Violated(Trace<S, A>),
    /// An example for a `sometimes` property: a path to a state where the condition holds.
    Found(Trace<S, A>),
    /// No state where a `sometimes` property's condition holds is reachable (the whole state
    /// space was explored).
    Unreachable,
    /// No example for a `sometimes` property was found in the part of the state space
    /// explored, which was not all of it.
    NotFound,
}

impl<S, A> Outcome<S, A> {
    /// Whether this outcome is a pass.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Holds | Self::Unrefuted | Self::Found(_))
    }

    /// The counterexample or example, if any.
    #[must_use]
    pub const fn trace(&self) -> Option<&Trace<S, A>> {
        match self {
            Self::Violated(trace) | Self::Found(trace) => Some(trace),
            Self::Holds | Self::Unrefuted | Self::Unreachable | Self::NotFound => None,
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bfs { threads: 1 } => f.write_str("breadth-first search"),
            Self::Bfs { threads } => write!(f, "breadth-first search ({threads} threads)"),
            Self::Dfs => f.write_str("depth-first search"),
            Self::RandomWalk {
                walks,
                max_depth,
                seeds,
            } => write!(
                f,
                "random walk: {walks} walks of at most {max_depth} steps, {seeds}"
            ),
        }
    }
}

impl fmt::Display for SeedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Run { run_seed } => write!(
                f,
                "seeds from the run's stream (reproduce with SIMULATOR_SEED={run_seed})"
            ),
            Self::Explicit(seed) => write!(f, "seeds from seed {seed}"),
            Self::Replay(seed) => write!(f, "replaying walk seed {seed}"),
        }
    }
}

impl<S: fmt::Debug, A: fmt::Debug> fmt::Display for Report<S, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let all_states = f.alternate();
        let stats = &self.stats;
        writeln!(
            f,
            "{}: {} states, {} unique, max depth {}, {}, {:.3?}",
            self.strategy,
            stats.states,
            stats.unique,
            stats.max_depth,
            if self.complete {
                "complete"
            } else {
                "incomplete"
            },
            stats.duration,
        )?;
        for property in &self.properties {
            let label = format!("{} \"{}\"", property.expectation, property.name);
            match &property.outcome {
                Outcome::Holds => writeln!(f, "  PASS {label}: holds")?,
                Outcome::Unrefuted => {
                    writeln!(f, "  PASS {label}: no counterexample (incomplete)")?;
                }
                Outcome::Found(trace) => {
                    writeln!(f, "  PASS {label}: example in {} steps", trace.len())?;
                }
                Outcome::Unreachable => writeln!(f, "  FAIL {label}: unreachable")?,
                Outcome::NotFound => {
                    writeln!(f, "  FAIL {label}: no example found (incomplete)")?;
                }
                Outcome::Violated(trace) => {
                    writeln!(f, "  FAIL {label}: counterexample in {} steps", trace.len())?;
                    if let Some(seed) = trace.walk_seed {
                        writeln!(
                            f,
                            "    replay this walk with SIMULATOR_EXPLORE_WALK_SEED={seed} \
                             (and the same max depth)"
                        )?;
                    }
                    let shown = if all_states {
                        format!("{trace:#}")
                    } else {
                        trace.to_string()
                    };
                    for line in shown.lines() {
                        writeln!(f, "    {line}")?;
                    }
                }
            }
        }
        Ok(())
    }
}
