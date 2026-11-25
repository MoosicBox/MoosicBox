# switchy_cargo

Checks that everything a switchy simulation links runs deterministically under it.

A simulation is a pure function of its seed only if the code it runs reaches clocks,
randomness, threads, files, the environment and process-wide state through what the
simulation controls (the switchy crates). `switchy_cargo` checks that for everything the
simulation links, found from its dependency graph (`cargo metadata`, normal dependencies, for
the host) rather than listed by hand.

## Local crates

Every local crate in the graph (the workspace's and other path dependencies; proc macros
excepted) is parsed (`syn`) from its roots, following `mod` declarations as rustc does
(`#[path]` and modules declared inside macros included), so the scan reads exactly the files
the crate compiles. Code whose `#[cfg]` is false for a simulation build is skipped: `test`,
and features the crate does not have enabled in the simulation's build. A `cfg` it cannot
decide (the operating system, `debug_assertions`) is scanned on both sides.

Every path is resolved before it is judged (`use` groups, aliases, globs, `self`/`super`/
`crate`, re-exports through the crate's own modules), so `use std::sync::Mutex as Lock;` or a
`pub use` elsewhere cannot hide a primitive. Macro invocations are scanned as tokens. It
reports:

- a primitive no seed controls (std threads, locks, atomics, clocks, files, environment,
  process id; tokio's runtime, `spawn`, `time`, `select!` and `watch`; `rand`, `new_v4`,
  `HashMap`, `HashSet`, ordering by address) reached other than through a `switchy_` crate,
  once per import;
- process-wide mutable state (a `static` lock, atomic or lazy value, and `thread_local!`);
- code chosen by operating system outside the files allowed to.

A line that uses the std version on purpose says why: `// simulation-safe: <reason>` (on it
or the line above). `// native-only: begin` ... `// native-only: end` marks native-only lines
the `cfg`s do not. `SWITCHY_CARGO_SHOW_UNREACHED=1` lists the source files no `mod` reaches
(tests, other platforms, dead files).

A crate states what simulations run of it in its manifest; without the table it is scanned
whole, so a crate a simulation starts linking is checked from the start:

```toml
[package.metadata.switchy.simulation]
# Linked, but never run inside a run:
runs = false
reason = "configuration types; files are read before any run starts"

# Or, for a crate that runs:
only = ["plugin_state.rs"]      # only these files run (prefixes under src/)
native = ["platform/unix/"]     # native backends, never built for simulation
exempt = ["ids.rs"]             # the controlled versions themselves
os-named-in = ["platform/"]     # where naming an operating system is allowed
forbid = ["Session::new("]      # more patterns, for this crate
```

Rules that name missing files are errors, so a rename cannot leave one covering nothing.

## External crates

Every crate from a registry or git repository the simulation links must be on the audit list
in the workspace's `determinism.toml`, as with `cargo vet`: a new crate, or a version outside
its entry, fails the check until it is audited.

```toml
forbid = ["my_crate::real_clock"]   # forbidden in every scanned crate

[[audit]]
name = "uuid"
version = "^1"
verdict = "restricted"   # pure | controlled | restricted | not-run | unaudited
reason = "new_v4 reads OS randomness; the shared rules forbid it"
```

`not-run` is a crate the simulation links but never runs (a transport no run starts, say): it
need not be read, and like a local crate's `runs = false` it is checked against coverage
(`--coverage`).

`--baseline REASON` prints `unaudited` entries, pinned to exact versions, for the crates the
list does not cover, to accept them as they are and audit them later.

`--scan-external` scans every external crate by the shared rules instead, with the features
the simulation enables, and prints each one's finding count (`--verbose` shows the first
few): evidence for an audit, not a verdict, since the shared rules do not know a crate's own
controlled paths.

## Running it

```text
switchy_cargo --manifest-path simulation/Cargo.toml -p my-simulation
```

or from code, with `switchy_cargo::Check`.
