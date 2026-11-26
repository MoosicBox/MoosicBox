# switchy_clock_probe

Counts the real clock reads a simulation run makes.

A simulation run's clock is the simulator's (`switchy_time`). The determinism checks forbid
std's clocks in the code a simulation runs, and audit the crates it links, but a dependency can
still call the C library's clock itself. This library finds out: loaded into a simulation's
process, it stands in for `clock_gettime`, `gettimeofday` and `time`, passes every call on to
the real function, and counts those made by a thread in a simulation run, by call stack.

It is never linked (its `clock_gettime` would replace the C library's in whatever linked it):

```sh
cargo build --release -p switchy_clock_probe
# Linux
LD_PRELOAD=target/release/libswitchy_clock_probe.so SIMULATOR_CLOCK_READS=report ./my-simulation
# macOS
DYLD_INSERT_LIBRARIES=target/release/libswitchy_clock_probe.dylib SIMULATOR_CLOCK_READS=report ./my-simulation
```

simvar drives it (`SIMULATOR_CLOCK_READS=report` or `fail`, and `SIMULATOR_CLOCK_READS_ALLOW`
for callers whose reads never reach a run): see simvar's `clock_reads` module. Build the
simulation with symbols and line tables to see who read the clock
(`CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`).

On macOS it does not stand in for `mach_absolute_time`, which the system itself calls while it
sets up a thread's storage; Rust's `Instant` and `SystemTime` use `clock_gettime` there.
