# switchy_cargo

Checks that everything a switchy simulation links runs deterministically under it.

A simulation is a pure function of its seed only if the code it runs reaches clocks,
randomness, threads, files, the environment and process-wide state through what the
simulation controls (the switchy crates). `switchy_cargo` checks that for everything the
simulation links, found from its dependency graph (`cargo metadata`, normal dependencies, for
the host) rather than listed by hand.

## Local crates

Every local crate in the graph (the workspace's and other path dependencies; proc macros
excepted) is scanned line by line. A line is reported when it:

- uses a primitive no seed controls (std threads, locks, atomics, clocks, files, environment,
  process id; tokio's runtime, `spawn`, `time`, `select!` and `watch`; `rand`, `new_v4`,
  `HashMap`, `HashSet`, ordering by address) other than through a `switchy_` crate;
- imports one in a group (`use std::{fs, sync::{Arc, Mutex}};`);
- declares process-wide mutable state (a `static` lock, atomic or lazy value);
- chooses code by operating system outside the files allowed to.

Test code is skipped: `#[cfg(test)]` items, the modules they load, and `tests.rs` /
`*_tests.rs`. A line that uses the std version on purpose says why:
`// simulation-safe: <reason>`.

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
verdict = "restricted"   # pure | controlled | restricted | unaudited
reason = "new_v4 reads OS randomness; the shared rules forbid it"
```

`--baseline REASON` prints `unaudited` entries, pinned to exact versions, for the crates the
list does not cover, to accept them as they are and audit them later.

## Running it

```text
switchy_cargo --manifest-path simulation/Cargo.toml -p my-simulation
```

or from code, with `switchy_cargo::Check`.
