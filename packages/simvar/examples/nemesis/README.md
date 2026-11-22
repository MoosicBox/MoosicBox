# Simvar Nemesis Example

Fault injection with simvar: three replicas of a counter exchange their values
over the simulated network while a *nemesis* client partitions the network
(symmetrically and one way), isolates replicas, degrades links (latency, loss,
duplication, reordering) and bounces replicas. A crash hook throws away each
crashed replica's unsynced disk writes.

Once the nemesis is done it heals everything, waits for every replica to run
again, and checks that they converge on one value no smaller than any value
ever synced.

Every fault and delivery comes from the run's seed, so a failing seed replays
exactly.

## Running

```bash
cargo run -p simvar_nemesis_example
SIMULATOR_RUNS=20 SIMULATOR_VERIFY_DETERMINISM=1 NO_TUI=1 cargo run -p simvar_nemesis_example
```
