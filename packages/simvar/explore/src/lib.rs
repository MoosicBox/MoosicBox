//! Model checking for simvar: explore every state of a small model, or random walks through
//! a large one, and check its properties on each.
//!
//! A system under test implements [`Model`]: initial states, the actions enabled in a state,
//! and the state each action leads to. Its [`Properties`] say what must hold: `always`
//! (safety invariants), `sometimes` (reachability) and `eventually` (bounded liveness on
//! terminating paths). A [`Checker`] then explores the model:
//!
//! * [`Checker::bfs`]: exhaustive breadth-first search; counterexamples are shortest
//! * [`Checker::bfs_parallel`]: the same, with worker threads expanding each level
//! * [`Checker::dfs`]: exhaustive depth-first search
//! * [`Checker::random_walk`]: seeded random walks for state spaces too large to exhaust
//!
//! All of them deduplicate states by fingerprint (after the model's optional symmetry
//! reduction, [`Model::canonicalize`]), honour depth and state-count bounds and the model's
//! [`Model::within_boundary`], and return a [`Report`] whose counterexamples are [`Trace`]s:
//! real, replayable paths from an initial state ([`Trace::verify`], [`replay`]).
//!
//! Exploration is deterministic. Random walks draw their seeds from the simulation run's
//! `simvar_explore` stream ([`switchy_simulation::stream`]), so a run reproduces from
//! `SIMULATOR_SEED` like any other simvar simulation, and each walk also reproduces from its
//! own seed ([`Checker::replay_walk`], `SIMULATOR_EXPLORE_WALK_SEED`).
//!
//! # Example
//!
//! Two processes guard a critical section with flags, but check the other's flag before
//! raising their own, so both can enter. Breadth-first search finds the shortest way there:
//!
//! ```rust
//! use simvar_explore::{Checker, Model, Outcome, Properties};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! enum Pc { Idle, Checked, Critical }
//!
//! struct NaiveMutex;
//!
//! impl Model for NaiveMutex {
//!     type State = [(Pc, bool); 2]; // per process: program counter, flag
//!     type Action = usize;          // which process steps
//!
//!     fn init_states(&self) -> Vec<Self::State> {
//!         vec![[(Pc::Idle, false); 2]]
//!     }
//!
//!     fn actions(&self, _state: &Self::State, actions: &mut Vec<usize>) {
//!         actions.extend([0, 1]);
//!     }
//!
//!     fn next_state(&self, state: &Self::State, &me: &usize) -> Option<Self::State> {
//!         let mut next = *state;
//!         let other_flag = state[1 - me].1;
//!         next[me] = match state[me] {
//!             (Pc::Idle, _) if !other_flag => (Pc::Checked, false),
//!             (Pc::Checked, _) => (Pc::Critical, true),
//!             (Pc::Critical, _) => (Pc::Idle, false),
//!             _ => return None, // waiting
//!         };
//!         Some(next)
//!     }
//!
//!     fn properties(&self, properties: &mut Properties<Self>) {
//!         properties.always("mutual exclusion", |_, s| {
//!             !(s[0].0 == Pc::Critical && s[1].0 == Pc::Critical)
//!         });
//!     }
//! }
//!
//! let report = Checker::new().bfs(&NaiveMutex);
//! assert!(!report.is_success());
//! let Outcome::Violated(trace) = &report.property("mutual exclusion").unwrap().outcome else {
//!     unreachable!()
//! };
//! assert_eq!(trace.len(), 4); // both check, then both enter
//! trace.verify(&NaiveMutex).unwrap();
//! println!("{report}");
//! ```
//!
//! # From a simulation binary
//!
//! ```rust,no_run
//! # use simvar_explore::{Checker, Model};
//! # struct Cluster;
//! # impl Model for Cluster {
//! #     type State = u8;
//! #     type Action = ();
//! #     fn init_states(&self) -> Vec<u8> { vec![0] }
//! #     fn actions(&self, _: &u8, _: &mut Vec<()>) {}
//! #     fn next_state(&self, _: &u8, _: &()) -> Option<u8> { None }
//! # }
//! fn main() {
//!     // SIMULATOR_SEED, SIMULATOR_EXPLORE_WALKS, SIMULATOR_EXPLORE_WALK_SEED, ... apply.
//!     let report = Checker::from_env().max_depth(50).random_walk(&Cluster);
//!     println!("{report}");
//!     if !report.is_success() {
//!         std::process::exit(1);
//!     }
//! }
//! ```
//!
//! # Limitations
//!
//! * `eventually` properties only find counterexamples that end in a terminal state;
//!   cycles that avoid the condition forever are not detected, and fairness is not modelled
//! * Visited states are kept as 128-bit fingerprints: a collision (vanishingly unlikely)
//!   would skip a state, never report a false counterexample
//! * A model may have at most 128 properties

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod bfs;
mod checker;
mod common;
mod dfs;
mod model;
mod report;
mod trace;
mod walk;

pub use checker::{Checker, DEFAULT_WALK_DEPTH, DEFAULT_WALKS};
pub use common::MAX_PROPERTIES;
pub use model::{Condition, Expectation, Model, Properties, Property};
pub use report::{ModelReport, Outcome, PropertyReport, Report, SeedSource, Stats, Strategy};
pub use trace::{ReplayError, ReplayErrorKind, Trace, replay};
pub use walk::STREAM;
