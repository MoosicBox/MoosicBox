# Simvar Threads Example

A bank whose tellers are real threads, run deterministically by simvar and
`switchy_thread`. Each run moves money between accounts from several simulated
threads; the seed alone decides how they interleave, so a failing run replays
exactly.

```bash
SIMULATOR_RUNS=20 NO_TUI=1 cargo run -p simvar_threads_example
# Check that every run replays identically:
SIMULATOR_VERIFY_DETERMINISM=1 SIMULATOR_RUNS=20 cargo run -p simvar_threads_example
# The same tellers as spawn_blocking work awaited by the async client:
BANK_BLOCKING=1 SIMULATOR_VERIFY_DETERMINISM=1 SIMULATOR_RUNS=20 cargo run -p simvar_threads_example
# A racy bank loses money; shrink the failing run's thread schedule and replay it:
BANK_RACY=1 SIMULATOR_SHRINK=1 SIMULATOR_RUNS=100 cargo run -p simvar_threads_example
```
