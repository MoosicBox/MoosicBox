//! Depth-first search with a fingerprint visited set, keeping the current path so every
//! discovery's trace is at hand without replaying.

use std::{collections::BTreeMap, time::Instant};

use crate::{
    Checker, Model, ModelReport, Stats, Strategy, Trace,
    common::{Discoveries, Mask, Props, expand, fingerprint},
};

/// A state waiting to be expanded, with the action that reached it.
struct Frame<S, A> {
    state: S,
    pending: Mask,
    depth: usize,
    action: Option<A>,
}

#[allow(clippy::too_many_lines)] // one search loop; splitting it would scatter its state
pub fn run<M: Model>(checker: &Checker, model: &M) -> ModelReport<M> {
    let started = Instant::now();
    let props = Props::new(model);
    let mut discoveries = Discoveries::new(&props, checker.stop_on_failure);
    let mut stats = Stats::default();
    // Each visited fingerprint with the shallowest depth it was reached at.
    let mut visited = BTreeMap::new();
    let mut truncated = false;
    let mut stop = false;
    let at_limit = |unique: u64| checker.max_states.is_some_and(|max| unique >= max);

    let mut stack = Vec::new();
    for state in model.init_states() {
        if !model.within_boundary(&state) {
            continue;
        }
        stats.states += 1;
        let holds = props.eval(model, &state);
        let pending = props.initial_pending(holds);
        let key = fingerprint(model, &state, pending);
        if visited.contains_key(&key) {
            continue;
        }
        if at_limit(stats.unique) {
            truncated = true;
            stop = true;
            break;
        }
        visited.insert(key, 0);
        stats.unique += 1;
        let discovered = discoveries.on_state(holds);
        if discovered != 0 {
            discoveries.record(discovered, &path_to(&[], &[], state.clone(), None));
            if discoveries.done() {
                stop = true;
                break;
            }
        }
        stack.push(Frame {
            state,
            pending,
            depth: 0,
            action: None,
        });
    }
    stack.reverse();

    let mut path_states: Vec<M::State> = Vec::new();
    let mut path_actions: Vec<M::Action> = Vec::new();
    let mut actions = Vec::new();
    let mut children = Vec::new();
    while !stop && let Some(frame) = stack.pop() {
        path_states.truncate(frame.depth);
        path_actions.truncate(frame.depth.saturating_sub(1));
        if let Some(action) = frame.action {
            path_actions.push(action);
        }
        path_states.push(frame.state);
        let current = path_states.last().expect("just pushed");

        let expansion = expand(model, &props, current, frame.pending, &mut actions);
        if expansion.terminal {
            let discovered = discoveries.on_terminal(frame.pending);
            if discovered != 0 {
                discoveries.record(
                    discovered,
                    &Trace {
                        states: path_states.clone(),
                        actions: path_actions.clone(),
                        walk_seed: None,
                    },
                );
                if discoveries.done() {
                    break;
                }
            }
        }
        let depth = frame.depth + 1;
        for successor in expansion.successors {
            stats.states += 1;
            let seen = visited.get(&successor.key).copied();
            if seen.is_some_and(|old| checker.max_depth.is_none() || old <= depth) {
                continue;
            }
            if checker.max_depth.is_some_and(|max| frame.depth >= max) {
                truncated = true;
                continue;
            }
            if seen.is_none() {
                if at_limit(stats.unique) {
                    truncated = true;
                    stop = true;
                    break;
                }
                stats.unique += 1;
            }
            visited.insert(successor.key, depth);
            stats.max_depth = stats.max_depth.max(depth);
            // A state reached again by a shorter path was already checked.
            let discovered = if seen.is_none() {
                discoveries.on_state(successor.holds)
            } else {
                0
            };
            if discovered != 0 {
                let trace = path_to(
                    &path_states,
                    &path_actions,
                    successor.state.clone(),
                    Some(successor.action.clone()),
                );
                discoveries.record(discovered, &trace);
                if discoveries.done() {
                    stop = true;
                    break;
                }
            }
            children.push(Frame {
                state: successor.state,
                pending: successor.pending,
                depth,
                action: Some(successor.action),
            });
        }
        // Explore the first action's successor first.
        while let Some(child) = children.pop() {
            stack.push(child);
        }
    }

    let complete = stack.is_empty() && !truncated && !stop && !discoveries.done();
    discoveries.into_report(Strategy::Dfs, stats, started, complete)
}

/// The current path extended by `action` to `state`.
fn path_to<S: Clone, A: Clone>(
    states: &[S],
    actions: &[A],
    state: S,
    action: Option<A>,
) -> Trace<S, A> {
    let mut trace = Trace {
        states: states.to_vec(),
        actions: actions.to_vec(),
        walk_seed: None,
    };
    trace.states.push(state);
    trace.actions.extend(action);
    trace
}
