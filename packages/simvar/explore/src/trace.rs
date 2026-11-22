//! Paths through a model: counterexamples and examples, and replaying them.

use std::fmt;

use crate::Model;

/// A path through a model: an initial state and the actions taken from it, with every state
/// along the way.
///
/// `states` always has one more element than `actions`: `states[i + 1]` is the result of
/// taking `actions[i]` in `states[i]`. Display prints the actions and the final state;
/// the alternate form (`{:#}`) prints every state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace<S, A> {
    /// The states along the path, starting with the initial state.
    pub states: Vec<S>,
    /// The actions taken, one fewer than the states.
    pub actions: Vec<A>,
    /// For a path found by a random walk, the walk's seed (replay it with
    /// [`Checker::replay_walk`](crate::Checker::replay_walk) or
    /// `SIMULATOR_EXPLORE_WALK_SEED`).
    pub walk_seed: Option<u64>,
}

impl<S, A> Trace<S, A> {
    /// The path's initial state.
    ///
    /// # Panics
    ///
    /// * If the trace has no states (checkers never produce one)
    #[must_use]
    pub fn init(&self) -> &S {
        self.states.first().expect("a trace has an initial state")
    }

    /// The state the path ends in.
    ///
    /// # Panics
    ///
    /// * If the trace has no states (checkers never produce one)
    #[must_use]
    pub fn last(&self) -> &S {
        self.states.last().expect("a trace has an initial state")
    }

    /// The number of actions in the path.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the path is just an initial state.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Each action with the state it led to, in order.
    pub fn steps(&self) -> impl Iterator<Item = (&A, &S)> {
        self.actions.iter().zip(self.states.iter().skip(1))
    }
}

impl<S: Clone + Eq, A> Trace<S, A> {
    /// Replay the path through `model` and check that it yields exactly the recorded states.
    ///
    /// # Errors
    ///
    /// * [`ReplayErrorKind::Ignored`] if an action yields no next state
    /// * [`ReplayErrorKind::Diverged`] if an action yields a different state than recorded
    pub fn verify<M>(&self, model: &M) -> Result<(), ReplayError>
    where
        M: Model<State = S, Action = A>,
    {
        let mut state = self.init().clone();
        for (step, (action, recorded)) in self.steps().enumerate() {
            let next = model.next_state(&state, action).ok_or(ReplayError {
                step,
                kind: ReplayErrorKind::Ignored,
            })?;
            if next != *recorded {
                return Err(ReplayError {
                    step,
                    kind: ReplayErrorKind::Diverged,
                });
            }
            state = next;
        }
        Ok(())
    }
}

impl<S: fmt::Debug, A: fmt::Debug> fmt::Display for Trace<S, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let all_states = f.alternate();
        writeln!(f, "init: {:?}", self.init())?;
        let last = self.len();
        for (index, (action, state)) in self.steps().enumerate() {
            let step = index + 1;
            writeln!(f, "{step:>4}. {action:?}")?;
            if all_states || step == last {
                writeln!(f, "      -> {state:?}")?;
            }
        }
        Ok(())
    }
}

/// Replay `actions` through `model` from `init`, returning every state along the way
/// (starting with `init`, so one more than the actions).
///
/// The actions are not checked against [`Model::actions`]; only [`Model::next_state`] is
/// consulted.
///
/// # Errors
///
/// * [`ReplayErrorKind::Ignored`] if an action yields no next state
pub fn replay<M: Model>(
    model: &M,
    init: &M::State,
    actions: &[M::Action],
) -> Result<Vec<M::State>, ReplayError> {
    let mut states = Vec::with_capacity(actions.len() + 1);
    let mut state = init.clone();
    for (step, action) in actions.iter().enumerate() {
        let next = model.next_state(&state, action).ok_or(ReplayError {
            step,
            kind: ReplayErrorKind::Ignored,
        })?;
        states.push(std::mem::replace(&mut state, next));
    }
    states.push(state);
    Ok(states)
}

/// Why a path could not be replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayError {
    /// The index of the action that failed.
    pub step: usize,
    /// What went wrong.
    pub kind: ReplayErrorKind,
}

/// What went wrong replaying a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayErrorKind {
    /// The action yielded no next state.
    Ignored,
    /// The action yielded a different state than the path recorded (the model is not
    /// deterministic, or changed since the path was found).
    Diverged,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ReplayErrorKind::Ignored => {
                write!(f, "action {} yielded no next state", self.step + 1)
            }
            ReplayErrorKind::Diverged => write!(
                f,
                "action {} yielded a different state than recorded",
                self.step + 1
            ),
        }
    }
}

impl std::error::Error for ReplayError {}
