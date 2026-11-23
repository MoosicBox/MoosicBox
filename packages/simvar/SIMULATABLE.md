# Making a crate simulatable

A simvar run is meant to be a pure function of its inputs: the seed, the simulator's
`SIMULATOR_*` environment, the build, and for guided or explored runs the recorded profile or
schedule. This page lists what that requires of the code a simulation runs, how each
requirement is checked, and how to state an exemption.

## What simulated code must use

Everything that would make two runs of one seed differ goes through switchy, whose crates are
std or tokio natively, at no cost, and simulated under their `simulator` features:

| Instead of | Use |
| --- | --- |
| `std::time::{Instant, SystemTime}::now`, tokio's `time` | `switchy_time`, `switchy_async::time` |
| `std::thread::spawn`, std's locks, condvars, atomics in shared state | `switchy_thread` |
| tokio's runtime, `spawn`, `select!`, channels and locks | `switchy_async` |
| `std::fs` | `switchy_fs` |
| `std::env::var` | `switchy_env` |
| `rand`, `Uuid::new_v4` | `switchy_random`, `switchy_uuid` |
| `HashMap`, `HashSet` (iteration order follows a per-process seed) | `BTreeMap`, `BTreeSet` |
| `static` caches, registries and counters | per-run state (`switchy_simulation::state`) |

There are no simulation hooks or simulation-only `cfg` branches in product code: the facades
are the only requirement. A std lock that guards plain data and is never held across a switch
point (an `.await`, a switchy lock or channel) may stay, with a reason (below).

## The checks

### Every run's observations, verified

`SIMULATOR_VERIFY_DETERMINISM=process` replays each run alone in a fresh, perturbed process
(another address layout and hash seed, process-wide counters started elsewhere, no runs beside
it) and compares everything the two observed: the schedule, processes and terminal output, the
file tree with contents and times, random draws, the clock. A failure names the first
observation that differs, with the text around it; `SIMULATOR_DIGEST_BACKTRACE_AT` adds a
backtrace at that point. `=1` replays in the same process, more cheaply.

### Everything a simulation links, checked

`switchy_cargo` walks a simulation's dependency graph (`cargo metadata`, normal dependencies,
for the host):

- **Local crates** are parsed and scanned: every compiled file, `cfg`s evaluated for a
  simulation build, every path resolved through imports and re-exports. A crate states what
  simulations run of it in its manifest:

  ```toml
  [package.metadata.switchy.simulation]
  only = ["engine/", "model.rs"]  # only these files run in simulations
  native = ["platform/unix/"]     # native backends, never built for simulation
  exempt = ["ids.rs"]             # the controlled versions themselves
  os-named-in = ["platform/"]     # where naming an operating system is allowed
  forbid = ["Session::new("]      # more patterns, for this crate

  # Or, linked but never run inside a run:
  runs = false
  reason = "configuration types; files are read before any run starts"
  ```

  A line that uses a primitive on purpose says why: `// simulation-safe: <reason>` (on it or
  the line above). `// native-only: begin` … `// native-only: end` marks native-only lines the
  `cfg`s do not.
- **Crates from registries and git** must be on the simulation workspace's
  `determinism.toml` audit list, with a verdict (`pure`, `controlled`, `restricted`,
  `unaudited`) and a reason; a new crate or version fails until it is audited.
  `--scan-external` scans each external crate by the shared rules, as evidence for an audit.
- **`runs = false` claims** are checked against coverage: build the simulation with
  `-C instrument-coverage`, run it, export the merged profile (`llvm-cov export
  -summary-only`), and `switchy_cargo --coverage EXPORT` fails if a file of such a crate ran.

### Sources intercepted

- **Randomness.** Build simulations with `--cfg getrandom_backend="custom"` (in the simulation
  workspace's `.cargo/config.toml` only, never a real build). `getrandom` 0.3 and 0.4, and so
  `uuid`, `rand`'s OS generator and hashers seeded through them, then draw from the run's
  seed (`switchy_random`'s custom backend), and panic outside a run. `getrandom` 0.2 cannot be
  redirected on systems it supports; its callers stay forbidden. Hashers seeded from addresses
  (`ahash` without `runtime-rng`, `foldhash`) cannot be intercepted either: never iterate their
  maps.
- **The environment.** Inside a run, `switchy_env` serves the run's own environment: the
  host's `SIMULATOR_*` variables, those the simulation lets through
  (`switchy_env::simulator::pass_through`), and the defaults; or exactly what the run set with
  `isolate`. A run reads the same environment wherever it executes, and runs never see each
  other's changes. Simulated processes (`switchy_pty`) start from a fixed environment.
- **Identifiers.** Task, runtime and lock identifiers are counted per run
  (`switchy_simulation::next_id`), so they never depend on what ran before in the process.

### Tripwires

- **Threads outside the scheduler.** Once simvar starts, a thread that reaches a switchy
  simulator without a context (a `std::thread::spawn` in simulated code, a dependency's thread
  pool) panics at that call, naming the problem, instead of running unscheduled with a clock
  and seed of its own. Start threads with `switchy_thread` (or `switchy_async`); a harness's
  own threads install a context with `switchy_simulation::set_current`.
- **Randomness outside a run** panics (see above).

## Exemptions

Exemptions live in manifests and the audit list, never in code paths: a file a simulation
does not run goes outside `only`; a native backend in `native`; a controlled implementation
in `exempt`; a crate linked but not run is `runs = false` with a reason (and the coverage check
holds it to that); a deliberate std primitive is a `// simulation-safe:` line with a reason;
an external crate gets an audit entry with a verdict and a reason.
