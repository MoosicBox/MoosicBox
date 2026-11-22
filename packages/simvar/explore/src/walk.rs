//! Seeded random walks: each walk draws its choices from its own generator, seeded with a
//! seed drawn from the run's `simvar_explore` stream (or [`Checker::seed`]), so any walk
//! replays from its seed alone.

use std::{collections::BTreeSet, time::Instant};

use switchy_simulation::Rng;

use crate::{
    Checker, DEFAULT_WALK_DEPTH, Model, ModelReport, SeedSource, Stats, Strategy, Trace,
    common::{Discoveries, Fingerprint, Props, fingerprint},
};

/// The run stream walk seeds are drawn from.
pub const STREAM: &str = "simvar_explore";

pub fn run<M: Model>(checker: &Checker, model: &M) -> ModelReport<M> {
    let started = Instant::now();
    let props = Props::new(model);
    let mut discoveries = Discoveries::new(&props, checker.stop_on_failure);
    let max_depth = checker.max_depth.unwrap_or(DEFAULT_WALK_DEPTH);
    let inits: Vec<_> = model
        .init_states()
        .into_iter()
        .filter(|s| model.within_boundary(s))
        .collect();
    let mut walker = Walker {
        model,
        props: &props,
        inits: &inits,
        max_depth,
        stats: Stats::default(),
        visited: BTreeSet::new(),
        actions: Vec::new(),
    };

    let (seeds, mut next_seed): (SeedSource, Box<dyn FnMut() -> u64>) =
        match (checker.walk_seed, checker.seed) {
            (Some(walk_seed), _) => (SeedSource::Replay(walk_seed), Box::new(move || walk_seed)),
            (None, Some(seed)) => {
                let mut rng = Rng::new(seed);
                (SeedSource::Explicit(seed), Box::new(move || rng.next_u64()))
            }
            (None, None) => {
                let context = switchy_simulation::current();
                let stream = context.stream(STREAM);
                (
                    SeedSource::Run {
                        run_seed: context.seed(),
                    },
                    Box::new(move || {
                        stream
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .next_u64()
                    }),
                )
            }
        };
    let walks = if checker.walk_seed.is_some() {
        1
    } else {
        checker.walks
    };

    for _ in 0..walks {
        walker.walk(next_seed(), &mut discoveries);
        if discoveries.done() {
            break;
        }
    }

    let strategy = Strategy::RandomWalk {
        walks: walker.stats.walks,
        max_depth,
        seeds,
    };
    discoveries.into_report(strategy, walker.stats, started, false)
}

struct Walker<'a, M: Model> {
    model: &'a M,
    props: &'a Props<M>,
    inits: &'a [M::State],
    max_depth: usize,
    stats: Stats,
    visited: BTreeSet<Fingerprint>,
    actions: Vec<M::Action>,
}

impl<M: Model> Walker<'_, M> {
    /// One walk from seed `seed`, recording what it discovers.
    fn walk(&mut self, seed: u64, discoveries: &mut Discoveries<'_, M>) {
        if self.inits.is_empty() {
            return;
        }
        self.stats.walks += 1;
        let mut rng = Rng::new(seed);
        let init = self.inits[rng.index(self.inits.len())].clone();
        let holds = self.props.eval(self.model, &init);
        let mut pending = self.props.initial_pending(holds);
        let mut trace = Trace {
            states: vec![init],
            actions: Vec::new(),
            walk_seed: Some(seed),
        };
        self.visit(&trace, holds, pending, discoveries);

        while trace.len() < self.max_depth && !discoveries.done() {
            let state = trace.last();
            self.actions.clear();
            self.model.actions(state, &mut self.actions);
            let mut chosen = None;
            let mut any_successor = false;
            while !self.actions.is_empty() {
                let action = self.actions.swap_remove(rng.index(self.actions.len()));
                let Some(next) = self.model.next_state(state, &action) else {
                    continue;
                };
                any_successor = true;
                if self.model.within_boundary(&next) {
                    chosen = Some((action, next));
                    break;
                }
            }
            let Some((action, next)) = chosen else {
                if !any_successor {
                    let discovered = discoveries.on_terminal(pending);
                    if discovered != 0 {
                        discoveries.record(discovered, &trace);
                    }
                }
                return;
            };
            let holds = self.props.eval(self.model, &next);
            pending &= !holds;
            trace.actions.push(action);
            trace.states.push(next);
            self.visit(&trace, holds, pending, discoveries);
        }
    }

    /// Count and check the state `trace` ends in.
    fn visit(
        &mut self,
        trace: &Trace<M::State, M::Action>,
        holds: u128,
        pending: u128,
        discoveries: &mut Discoveries<'_, M>,
    ) {
        self.stats.states += 1;
        self.stats.max_depth = self.stats.max_depth.max(trace.len());
        if self
            .visited
            .insert(fingerprint(self.model, trace.last(), pending))
        {
            self.stats.unique += 1;
        }
        let discovered = discoveries.on_state(holds);
        if discovered != 0 {
            discoveries.record(discovered, trace);
        }
    }
}
