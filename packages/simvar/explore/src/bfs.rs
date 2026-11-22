//! Breadth-first search, sequential or with worker threads expanding each level.
//!
//! Each level's states are expanded in chunks (one state at a time sequentially, many at
//! once in parallel) and their successors merged in order, so both find the same states,
//! counts and shortest paths. Only fingerprints are kept for visited states, with the
//! fingerprint of the state each was first reached from; a path is rebuilt by replaying the
//! model along that chain of fingerprints.

use std::{collections::BTreeMap, time::Instant};

use crate::{
    Checker, Model, ModelReport, Stats, Strategy, Trace,
    common::{Discoveries, Expansion, Fingerprint, Mask, Props, expand, fingerprint},
};

/// States expanded per thread in one parallel chunk.
const PER_THREAD: usize = 1024;

/// A visited state waiting to be expanded.
struct Entry<S> {
    state: S,
    pending: Mask,
    key: Fingerprint,
    depth: usize,
}

pub fn sequential<M: Model>(checker: &Checker, model: &M) -> ModelReport<M> {
    let props = Props::new(model);
    let mut actions = Vec::new();
    search(checker, model, &props, 1, 1, |chunk| {
        chunk
            .iter()
            .map(|e| expand(model, &props, &e.state, e.pending, &mut actions))
            .collect()
    })
}

pub fn parallel<M>(checker: &Checker, model: &M, threads: usize) -> ModelReport<M>
where
    M: Model + Sync,
    M::State: Send + Sync,
    M::Action: Send,
{
    let props = Props::new(model);
    let props = &props;
    search(
        checker,
        model,
        props,
        threads,
        threads * PER_THREAD,
        |chunk| {
            if threads == 1 || chunk.len() < 2 {
                let mut actions = Vec::new();
                return chunk
                    .iter()
                    .map(|e| expand(model, props, &e.state, e.pending, &mut actions))
                    .collect();
            }
            let per_thread = chunk.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let workers: Vec<_> = chunk
                    .chunks(per_thread)
                    .map(|part| {
                        scope.spawn(move || {
                            let mut actions = Vec::new();
                            part.iter()
                                .map(|e| expand(model, props, &e.state, e.pending, &mut actions))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                workers
                    .into_iter()
                    .flat_map(|worker| {
                        worker
                            .join()
                            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                    })
                    .collect()
            })
        },
    )
}

/// The search state shared by the merge steps.
struct Search<'a, 'p, M: Model> {
    checker: &'a Checker,
    model: &'a M,
    props: &'p Props<M>,
    /// Each visited state's fingerprint, with the fingerprint it was first reached from.
    visited: BTreeMap<Fingerprint, Option<Fingerprint>>,
    discoveries: Discoveries<'p, M>,
    stats: Stats,
    /// A state was left unexplored because of the depth or state limit.
    truncated: bool,
    stop: bool,
}

fn search<M, F>(
    checker: &Checker,
    model: &M,
    props: &Props<M>,
    threads: usize,
    chunk_size: usize,
    mut expand_chunk: F,
) -> ModelReport<M>
where
    M: Model,
    F: FnMut(&[Entry<M::State>]) -> Vec<Expansion<M::State, M::Action>>,
{
    let started = Instant::now();
    let mut search = Search {
        checker,
        model,
        props,
        visited: BTreeMap::new(),
        discoveries: Discoveries::new(props, checker.stop_on_failure),
        stats: Stats::default(),
        truncated: false,
        stop: false,
    };
    let mut frontier = search.init();
    while !frontier.is_empty() && !search.stop {
        let mut next = Vec::new();
        for chunk in frontier.chunks(chunk_size) {
            let expansions = expand_chunk(chunk);
            for (entry, expansion) in chunk.iter().zip(expansions) {
                search.merge(entry, expansion, &mut next);
                if search.stop {
                    break;
                }
            }
            if search.stop {
                break;
            }
        }
        frontier = next;
    }
    let complete = frontier.is_empty() && !search.truncated && !search.stop;
    let Search {
        discoveries, stats, ..
    } = search;
    discoveries.into_report(Strategy::Bfs { threads }, stats, started, complete)
}

impl<M: Model> Search<'_, '_, M> {
    fn init(&mut self) -> Vec<Entry<M::State>> {
        let mut frontier = Vec::new();
        for state in self.model.init_states() {
            if !self.model.within_boundary(&state) {
                continue;
            }
            self.stats.states += 1;
            let holds = self.props.eval(self.model, &state);
            let pending = self.props.initial_pending(holds);
            let key = fingerprint(self.model, &state, pending);
            if self.visited.contains_key(&key) {
                continue;
            }
            if self.at_state_limit() {
                break;
            }
            self.visited.insert(key, None);
            self.stats.unique += 1;
            let discovered = self.discoveries.on_state(holds);
            if discovered != 0 {
                let trace = Trace {
                    states: vec![state.clone()],
                    actions: Vec::new(),
                    walk_seed: None,
                };
                self.discoveries.record(discovered, &trace);
            }
            frontier.push(Entry {
                state,
                pending,
                key,
                depth: 0,
            });
            if self.discoveries.done() {
                self.stop = true;
                break;
            }
        }
        frontier
    }

    fn at_state_limit(&mut self) -> bool {
        if self
            .checker
            .max_states
            .is_some_and(|max| self.stats.unique >= max)
        {
            self.truncated = true;
            self.stop = true;
            return true;
        }
        false
    }

    /// Merge one expanded state's successors into the search.
    fn merge(
        &mut self,
        entry: &Entry<M::State>,
        expansion: Expansion<M::State, M::Action>,
        next: &mut Vec<Entry<M::State>>,
    ) {
        if expansion.terminal {
            let discovered = self.discoveries.on_terminal(entry.pending);
            if discovered != 0 {
                let trace = self.rebuild(entry.key);
                self.discoveries.record(discovered, &trace);
                if self.discoveries.done() {
                    self.stop = true;
                    return;
                }
            }
        }
        let depth = entry.depth + 1;
        for successor in expansion.successors {
            self.stats.states += 1;
            if self.visited.contains_key(&successor.key) {
                continue;
            }
            if self.checker.max_depth.is_some_and(|max| entry.depth >= max) {
                self.truncated = true;
                continue;
            }
            if self.at_state_limit() {
                return;
            }
            self.visited.insert(successor.key, Some(entry.key));
            self.stats.unique += 1;
            self.stats.max_depth = self.stats.max_depth.max(depth);
            let discovered = self.discoveries.on_state(successor.holds);
            if discovered != 0 {
                let trace = self.rebuild(successor.key);
                self.discoveries.record(discovered, &trace);
                if self.discoveries.done() {
                    self.stop = true;
                    return;
                }
            }
            next.push(Entry {
                state: successor.state,
                pending: successor.pending,
                key: successor.key,
                depth,
            });
        }
    }

    /// The path to the visited state with fingerprint `key`, rebuilt by replaying the model
    /// along the chain of fingerprints that first reached it.
    fn rebuild(&self, key: Fingerprint) -> Trace<M::State, M::Action> {
        let mut chain = vec![key];
        while let Some(Some(parent)) = self.visited.get(chain.last().expect("non-empty")) {
            chain.push(*parent);
        }
        chain.reverse();

        let (mut state, mut pending) = self
            .model
            .init_states()
            .into_iter()
            .filter(|s| self.model.within_boundary(s))
            .map(|s| {
                let pending = self.props.initial_pending(self.props.eval(self.model, &s));
                (s, pending)
            })
            .find(|(s, pending)| fingerprint(self.model, s, *pending) == chain[0])
            .unwrap_or_else(|| not_deterministic());
        let mut trace = Trace {
            states: vec![state.clone()],
            actions: Vec::new(),
            walk_seed: None,
        };
        let mut actions = Vec::new();
        for &target in &chain[1..] {
            let successor = expand(self.model, self.props, &state, pending, &mut actions)
                .successors
                .into_iter()
                .find(|s| s.key == target)
                .unwrap_or_else(|| not_deterministic());
            state = successor.state;
            pending = successor.pending;
            trace.actions.push(successor.action);
            trace.states.push(state.clone());
        }
        trace
    }
}

fn not_deterministic() -> ! {
    panic!(
        "simvar_explore: could not rebuild a path the search found; the model's \
         init_states/actions/next_state are not deterministic, or canonicalize is not a \
         symmetry of the model"
    )
}
