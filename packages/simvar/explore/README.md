# Simvar Explore

Model checking for simvar: explore every reachable state of a small model, or seeded
random walks through a large one, checking its properties on each state.

## Overview

A system under test implements `Model`, a nondeterministic state machine:

- `init_states()`: the states it can start in
- `actions(&state, &mut actions)`: the actions enabled in a state
- `next_state(&state, &action) -> Option<State>`: the state an action leads to (`None`: the
  action does nothing)
- `properties(&mut Properties)`: what must hold
    - `always`: a safety invariant, checked in every reached state
    - `sometimes`: a reachability claim; the checker shows a path to such a state
    - `eventually`: bounded liveness; every *terminating* path must reach such a state
- `within_boundary(&state)` (optional): bound an infinite model
- `canonicalize(&state)` (optional): symmetry reduction, for example sorting
  interchangeable nodes

A `Checker` explores it:

- `bfs`: exhaustive breadth-first search; counterexamples are as short as possible
- `bfs_parallel`: the same with worker threads, with identical results
- `dfs`: exhaustive depth-first search (with a depth bound, states reached again by a
  shorter path are re-explored, so nothing within the bound is missed)
- `random_walk`: seeded random walks, for state spaces too large to exhaust

Every checker deduplicates states by a 128-bit fingerprint, honours depth (`max_depth`) and
state-count (`max_states`) bounds, and returns a `Report`. Counterexamples are `Trace`s:
real paths from an initial state, replayable with `Trace::verify` or `replay`.

## Example

```rust
use simvar_explore::{Checker, Model, Properties};

/// Counts up by one or two, up to 10.
struct Counter;

impl Model for Counter {
    type State = u32;
    type Action = u32;

    fn init_states(&self) -> Vec<u32> {
        vec![0]
    }

    fn actions(&self, _state: &u32, actions: &mut Vec<u32>) {
        actions.extend([1, 2]);
    }

    fn next_state(&self, state: &u32, step: &u32) -> Option<u32> {
        Some(state + step)
    }

    fn within_boundary(&self, state: &u32) -> bool {
        *state <= 10
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties
            .always("at most 10", |_, s| *s <= 10)
            .sometimes("reaches 7", |_, s| *s == 7);
    }
}

let report = Checker::new().bfs(&Counter);
println!("{report}");
assert!(report.is_success());
```

See `examples/replicated_log.rs` for a leader replicating a log to two followers over a
lossy, reordering network with leader crashes and follower symmetry.

## Simulations

Random walks draw their seeds from the simulation run's `simvar_explore` stream
(`switchy_simulation`), so a run reproduces from `SIMULATOR_SEED` like any simvar
simulation, and every walk also reproduces alone from its own seed, which a failing walk's
report prints. `Checker::from_env()` reads:

| Variable                       | Meaning                                          |
| ------------------------------ | ------------------------------------------------ |
| `SIMULATOR_SEED`               | The run's seed, from which walk seeds are drawn  |
| `SIMULATOR_EXPLORE_MAX_DEPTH`  | Most actions from an initial state (walk length) |
| `SIMULATOR_EXPLORE_MAX_STATES` | Most distinct states a search visits             |
| `SIMULATOR_EXPLORE_THREADS`    | Worker threads for `bfs_parallel`                |
| `SIMULATOR_EXPLORE_WALKS`      | Random walks to take                             |
| `SIMULATOR_EXPLORE_WALK_SEED`  | Replay the single walk with this seed            |

## Limitations

- `eventually` properties only detect counterexamples ending in a terminal state; cycles
  that avoid the condition forever are not detected, and fairness is not modelled
- A fingerprint collision (vanishingly unlikely with 128 bits) would skip a state; it can
  never produce a false counterexample
- A model may have at most 128 properties

## Installation

```toml
[dependencies]
simvar_explore = "0.5.0"
```

Or through the `simvar` facade's `explore` feature, as `simvar::explore`.
