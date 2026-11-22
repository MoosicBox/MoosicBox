//! What every checker shares: evaluating properties, fingerprinting states, expanding a
//! state into its successors, and keeping the first discovery for each property.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    time::Instant,
};

use crate::{
    Expectation, Model, Outcome, Properties, Property, PropertyReport, Report, Stats, Strategy,
    Trace,
};

/// The most properties a model may have: their results are kept as bits of a `u128`.
pub const MAX_PROPERTIES: usize = 128;

/// One bit per property: which conditions hold in a state, or which `eventually`
/// properties are still pending along a path.
pub type Mask = u128;

/// A 128-bit fingerprint of a (canonical) state and its pending `eventually` properties.
/// Two fingerprints collide with negligible probability; a collision would only make a
/// checker skip a state, never report a false counterexample (paths are replayed).
pub type Fingerprint = u128;

/// A model's properties, evaluated together.
pub struct Props<M: Model> {
    list: Vec<Property<M>>,
    eventually: Mask,
}

impl<M: Model> Props<M> {
    /// The model's properties.
    ///
    /// # Panics
    ///
    /// * If the model has more than [`MAX_PROPERTIES`] properties
    pub fn new(model: &M) -> Self {
        let mut properties = Properties::new();
        model.properties(&mut properties);
        let list = properties.into_vec();
        assert!(
            list.len() <= MAX_PROPERTIES,
            "a model may have at most {MAX_PROPERTIES} properties, it has {}",
            list.len()
        );
        let eventually = bits(&list, |p| p.expectation() == Expectation::Eventually);
        Self { list, eventually }
    }

    /// Which conditions hold in `state`.
    pub fn eval(&self, model: &M, state: &M::State) -> Mask {
        self.list
            .iter()
            .enumerate()
            .filter(|(_, p)| p.holds(model, state))
            .fold(0, |mask, (i, _)| mask | (1 << i))
    }

    /// The `eventually` properties still pending in an initial state where `holds` hold.
    pub const fn initial_pending(&self, holds: Mask) -> Mask {
        self.eventually & !holds
    }
}

fn bits<M: Model>(list: &[Property<M>], f: impl Fn(&Property<M>) -> bool) -> Mask {
    list.iter()
        .enumerate()
        .filter(|(_, p)| f(p))
        .fold(0, |mask, (i, _)| mask | (1 << i))
}

/// The fingerprint of `state` (after canonicalization) with `pending` properties.
pub fn fingerprint<M: Model>(model: &M, state: &M::State, pending: Mask) -> Fingerprint {
    let canonical = model.canonicalize(state);
    let half = |salt: u8| {
        let mut hasher = DefaultHasher::new();
        salt.hash(&mut hasher);
        canonical.hash(&mut hasher);
        pending.hash(&mut hasher);
        hasher.finish()
    };
    (u128::from(half(0)) << 64) | u128::from(half(1))
}

/// A state reached by one action, with its property results.
pub struct Successor<S, A> {
    pub action: A,
    pub state: S,
    pub holds: Mask,
    pub pending: Mask,
    pub key: Fingerprint,
}

/// A state's successors within the boundary, and whether it is terminal.
pub struct Expansion<S, A> {
    pub successors: Vec<Successor<S, A>>,
    /// No action yields a next state at all (not even one outside the boundary).
    pub terminal: bool,
}

/// Every successor of `state` (reached with `pending` properties pending), in action order.
pub fn expand<M: Model>(
    model: &M,
    props: &Props<M>,
    state: &M::State,
    pending: Mask,
    actions: &mut Vec<M::Action>,
) -> Expansion<M::State, M::Action> {
    actions.clear();
    model.actions(state, actions);
    let mut terminal = true;
    let mut successors = Vec::with_capacity(actions.len());
    for action in actions.drain(..) {
        let Some(next) = model.next_state(state, &action) else {
            continue;
        };
        terminal = false;
        if !model.within_boundary(&next) {
            continue;
        }
        let holds = props.eval(model, &next);
        let pending = pending & !holds;
        let key = fingerprint(model, &next, pending);
        successors.push(Successor {
            action,
            state: next,
            holds,
            pending,
            key,
        });
    }
    Expansion {
        successors,
        terminal,
    }
}

/// The first discovery for each property, and when a check may stop.
pub struct Discoveries<'p, M: Model> {
    props: &'p Props<M>,
    found: Vec<Option<Trace<M::State, M::Action>>>,
    undiscovered: Mask,
    failed: bool,
    stop_on_failure: bool,
}

impl<'p, M: Model> Discoveries<'p, M> {
    pub fn new(props: &'p Props<M>, stop_on_failure: bool) -> Self {
        let count = props.list.len();
        Self {
            props,
            found: vec![None; count],
            undiscovered: if count == MAX_PROPERTIES {
                Mask::MAX
            } else {
                (1 << count) - 1
            },
            failed: false,
            stop_on_failure,
        }
    }

    /// The properties for which a state where `holds` hold is a new discovery: an `always`
    /// property that does not hold, or a `sometimes` property that does.
    pub fn on_state(&self, holds: Mask) -> Mask {
        let mut mask = 0;
        for (i, p) in self.props.list.iter().enumerate() {
            let bit = 1 << i;
            if self.undiscovered & bit == 0 {
                continue;
            }
            let discovered = match p.expectation() {
                Expectation::Always => holds & bit == 0,
                Expectation::Sometimes => holds & bit != 0,
                Expectation::Eventually => false,
            };
            if discovered {
                mask |= bit;
            }
        }
        mask
    }

    /// The `eventually` properties for which reaching a terminal state with `pending` still
    /// pending is a new counterexample.
    pub const fn on_terminal(&self, pending: Mask) -> Mask {
        pending & self.undiscovered
    }

    /// Record `trace` as the discovery for every property in `mask`.
    pub fn record(&mut self, mask: Mask, trace: &Trace<M::State, M::Action>) {
        for (i, p) in self.props.list.iter().enumerate() {
            if mask & (1 << i) != 0 {
                self.found[i] = Some(trace.clone());
                if p.expectation() != Expectation::Sometimes {
                    self.failed = true;
                }
            }
        }
        self.undiscovered &= !mask;
    }

    /// Whether there is nothing more to learn: a failure was found and the check stops on
    /// failures, or every property has its discovery.
    pub const fn done(&self) -> bool {
        (self.failed && self.stop_on_failure) || (self.undiscovered == 0 && !self.found.is_empty())
    }

    /// The report, given whether the whole state space was explored.
    pub fn into_report(
        self,
        strategy: Strategy,
        mut stats: Stats,
        started: Instant,
        complete: bool,
    ) -> Report<M::State, M::Action> {
        stats.duration = started.elapsed();
        let properties = self
            .props
            .list
            .iter()
            .zip(self.found)
            .map(|(p, found)| {
                let sometimes = p.expectation() == Expectation::Sometimes;
                let outcome = match (found, sometimes, complete) {
                    (Some(trace), true, _) => Outcome::Found(trace),
                    (Some(trace), false, _) => Outcome::Violated(trace),
                    (None, true, true) => Outcome::Unreachable,
                    (None, true, false) => Outcome::NotFound,
                    (None, false, true) => Outcome::Holds,
                    (None, false, false) => Outcome::Unrefuted,
                };
                PropertyReport {
                    name: p.name().to_owned(),
                    expectation: p.expectation(),
                    outcome,
                }
            })
            .collect();
        Report {
            strategy,
            stats,
            complete,
            properties,
        }
    }
}
