# Switchy Simulation

Per-run state shared by the switchy simulators: a `Context` holding the run's
seed, independent named RNG streams, and one value per simulator type.

Each OS thread has a current context. Threads that were never given one get a
fresh context of their own, so unrelated threads stay independent.
`switchy_thread` installs a parent's context in the simulated threads it
spawns, so every thread of a run shares one seed, clock and RNG.

The PRNG is xoshiro256\*\* seeded through SplitMix64. Its output for a seed is
identical on every platform, so a recorded seed reproduces a simulation
anywhere. It is not cryptographically secure.

The crate depends only on `std`.

## Installation

```toml
[dependencies]
switchy_simulation = "0.5.0"
```

## Usage

```rust
use std::sync::Mutex;

#[derive(Default)]
struct Clock(Mutex<u64>);

// This run's clock, created on first use.
let clock = switchy_simulation::state::<Clock>();
*clock.0.lock().unwrap() += 1;

// An RNG stream named after the simulator: draws here never shift other streams.
let jitter = switchy_simulation::stream("my_simulator").lock().unwrap().below(10);

// Run code as part of a specific run.
let run = switchy_simulation::Context::new(42);
switchy_simulation::with_context(run, || assert_eq!(switchy_simulation::seed(), 42));
```

`SIMULATOR_SEED` fixes the process's initial seed; `gen_seed` derives further
run seeds from it.
