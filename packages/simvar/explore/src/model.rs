//! The [`Model`] trait a system implements to be explored, and its [`Property`]s.

use std::{borrow::Cow, fmt, hash::Hash};

/// A nondeterministic state machine to explore.
///
/// A model has one or more initial states ([`Model::init_states`]); in every state some
/// actions are enabled ([`Model::actions`]), and taking one yields the next state
/// ([`Model::next_state`]). Checkers explore the graph this defines and check the model's
/// [`Property`]s on every state they reach.
///
/// Every method must be deterministic: the same state and action must always yield the same
/// next state. Counterexamples are rebuilt by replaying actions, so a nondeterministic model
/// makes them unreliable (and breadth-first search panics when it cannot rebuild one).
///
/// # Examples
///
/// ```rust
/// use simvar_explore::{Checker, Model, Properties};
///
/// /// A counter that can be incremented or doubled, up to 20.
/// struct Counter;
///
/// impl Model for Counter {
///     type State = u32;
///     type Action = &'static str;
///
///     fn init_states(&self) -> Vec<u32> {
///         vec![1]
///     }
///
///     fn actions(&self, _state: &u32, actions: &mut Vec<&'static str>) {
///         actions.extend(["increment", "double"]);
///     }
///
///     fn next_state(&self, state: &u32, action: &&'static str) -> Option<u32> {
///         let next = if *action == "increment" { state + 1 } else { state * 2 };
///         (next <= 20).then_some(next)
///     }
///
///     fn properties(&self, properties: &mut Properties<Self>) {
///         properties
///             .always("at most 20", |_, state| *state <= 20)
///             .sometimes("reaches 16", |_, state| *state == 16);
///     }
/// }
///
/// let report = Checker::new().bfs(&Counter);
/// assert!(report.is_success(), "{report}");
/// // 1 -> 2 -> 4 -> 8 -> 16 is the shortest way to 16.
/// let example = report.property("reaches 16").unwrap().outcome.trace().unwrap();
/// assert_eq!(example.len(), 4);
/// ```
pub trait Model {
    /// A state of the system. Two states that are equal must behave identically.
    type State: Clone + Eq + Hash + fmt::Debug;

    /// A step the system can take from a state.
    type Action: Clone + fmt::Debug;

    /// The states the system can start in.
    fn init_states(&self) -> Vec<Self::State>;

    /// Push the actions enabled in `state` onto `actions` (which arrives empty).
    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>);

    /// The state after taking `action` in `state`, or `None` when the action turns out to do
    /// nothing (for example a message whose delivery is ignored). A state in which no action
    /// yields a next state is *terminal*.
    fn next_state(&self, state: &Self::State, action: &Self::Action) -> Option<Self::State>;

    /// Add the properties to check on every reached state to `properties` (which arrives
    /// empty). Called once per check.
    fn properties(&self, _properties: &mut Properties<Self>)
    where
        Self: Sized,
    {
    }

    /// Whether `state` belongs to the part of the state space to explore. States outside
    /// the boundary are neither checked nor expanded (but their predecessor is not terminal).
    /// Use it to bound otherwise infinite models, for example by a maximum term or log length.
    fn within_boundary(&self, _state: &Self::State) -> bool {
        true
    }

    /// The representative of `state`'s equivalence class, for symmetry reduction: checkers
    /// treat two states with the same representative as already visited. For a cluster of
    /// interchangeable nodes this is typically the state with its nodes sorted.
    ///
    /// It must be a real symmetry: equivalent states must have equivalent successors and the
    /// same property results, or checkers miss states (and counterexamples may not rebuild).
    /// Checkers still explore and report the actual states, never the representatives, so
    /// every counterexample is a real path through the model.
    fn canonicalize<'s>(&self, state: &'s Self::State) -> Cow<'s, Self::State> {
        Cow::Borrowed(state)
    }
}

/// What a [`Property`] claims about its condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Expectation {
    /// The condition holds in every reachable state (a safety invariant). A reachable state
    /// where it does not is a counterexample.
    Always,
    /// The condition holds in at least one reachable state. A state where it does is an
    /// example; finding none is a failure.
    Sometimes,
    /// Every path eventually reaches a state where the condition holds (bounded liveness).
    ///
    /// Only *finite* maximal paths are checked: a counterexample is a path to a terminal
    /// state (one where no action yields a next state) along which the condition never held.
    /// Infinite paths (cycles that avoid the condition forever) are not detected, so this is
    /// sound (every reported counterexample is real) but incomplete for models with cycles.
    /// Fairness is not modelled.
    Eventually,
}

impl fmt::Display for Expectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Always => "always",
            Self::Sometimes => "sometimes",
            Self::Eventually => "eventually",
        })
    }
}

/// The condition a [`Property`] tests on one state, given the model.
pub type Condition<M> = fn(&M, &<M as Model>::State) -> bool;

/// A named claim about a model's states, checked by every checker.
pub struct Property<M: Model> {
    name: Cow<'static, str>,
    expectation: Expectation,
    condition: Condition<M>,
}

impl<M: Model> Property<M> {
    /// A property with the given expectation.
    #[must_use]
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        expectation: Expectation,
        condition: Condition<M>,
    ) -> Self {
        Self {
            name: name.into(),
            expectation,
            condition,
        }
    }

    /// A safety invariant: `condition` holds in every reachable state.
    #[must_use]
    pub fn always(name: impl Into<Cow<'static, str>>, condition: Condition<M>) -> Self {
        Self::new(name, Expectation::Always, condition)
    }

    /// A reachability claim: `condition` holds in some reachable state.
    #[must_use]
    pub fn sometimes(name: impl Into<Cow<'static, str>>, condition: Condition<M>) -> Self {
        Self::new(name, Expectation::Sometimes, condition)
    }

    /// A bounded liveness claim: every terminating path reaches a state where `condition`
    /// holds. See [`Expectation::Eventually`] for what is and is not checked.
    #[must_use]
    pub fn eventually(name: impl Into<Cow<'static, str>>, condition: Condition<M>) -> Self {
        Self::new(name, Expectation::Eventually, condition)
    }

    /// The property's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the property claims.
    #[must_use]
    pub const fn expectation(&self) -> Expectation {
        self.expectation
    }

    /// Whether the condition holds in `state`.
    #[must_use]
    pub fn holds(&self, model: &M, state: &M::State) -> bool {
        (self.condition)(model, state)
    }
}

/// The properties a [`Model`] declares in [`Model::properties`].
///
/// Adding them through these methods (rather than building [`Property`] values) lets the
/// compiler infer the conditions' parameter types.
pub struct Properties<M: Model> {
    list: Vec<Property<M>>,
}

impl<M: Model> Default for Properties<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> Properties<M> {
    /// No properties.
    #[must_use]
    pub const fn new() -> Self {
        Self { list: Vec::new() }
    }

    /// Add a safety invariant: `condition` holds in every reachable state.
    pub fn always(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        condition: Condition<M>,
    ) -> &mut Self {
        self.push(Property::always(name, condition))
    }

    /// Add a reachability claim: `condition` holds in some reachable state.
    pub fn sometimes(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        condition: Condition<M>,
    ) -> &mut Self {
        self.push(Property::sometimes(name, condition))
    }

    /// Add a bounded liveness claim; see [`Expectation::Eventually`].
    pub fn eventually(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        condition: Condition<M>,
    ) -> &mut Self {
        self.push(Property::eventually(name, condition))
    }

    /// Add `property`.
    pub fn push(&mut self, property: Property<M>) -> &mut Self {
        self.list.push(property);
        self
    }

    /// The properties added so far, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[Property<M>] {
        &self.list
    }

    /// The properties added, in order.
    #[must_use]
    pub fn into_vec(self) -> Vec<Property<M>> {
        self.list
    }
}

impl<M: Model> fmt::Debug for Properties<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.list).finish()
    }
}

impl<M: Model> Clone for Property<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            expectation: self.expectation,
            condition: self.condition,
        }
    }
}

impl<M: Model> fmt::Debug for Property<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("name", &self.name)
            .field("expectation", &self.expectation)
            .finish_non_exhaustive()
    }
}
