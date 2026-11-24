# Single-root driver and local executor: scope and verification

## Opt-in task scheduling

`owned::Executor` builds on `owned::block_on`. Spawn local tasks before driving a root; there is no ambient spawner or background thread. For example:

```rust
use switchy_async::owned::Executor;
let mut runtime = Executor::default();
let task = runtime.spawn(async { 17 });
assert_eq!(runtime.run_until(task), Ok(17));
```

The caller owns the task registry; only wake tickets cross threads. Duplicate notifications coalesce, ready tickets run FIFO, and a separate root notification prevents unrelated queue turns from polling a sleeping root. A task survives handle drop, can be aborted between polls, and reports its value, cancellation, or polling panic through a local join future. Dropping the executor releases pending futures and resolves outstanding joins. Panicking destructors are not isolated. No change to default routing, public backend aliases or dependencies.

### Scheduler verification

The shared lifecycle probe uses a five-second watchdog per backend. It covers external-thread duplicate wake and delivery, detached progress, aborting a sleeping task twice, capture destruction, joining after shutdown and a retained task waker fired after shutdown. It runs against the candidate, Tokio current-thread `LocalSet`, and the simulator (`shutdown_now` for its explicit shutdown protocol). Only the candidate asserts exact FIFO order, poll-panic isolation and abort after completion. Three independent test-process launches check the same nine-poll trace; this is deterministic replay for a controlled ready queue, not for racing OS threads.

Used the isolated baseline workspace described below. Copied `src/owned.rs`, `src/owned/scheduler.rs` and `tests/owned_scheduler_contract.rs` to the same relative paths there. Inside the checkout's `nix develop --offline --no-write-lock-file -c sh`:

```sh
set -e
cd /tmp/switchy-async-owned-source
export CARGO_HOME=/tmp/switchy-owned-cargo CARGO_TARGET_DIR=/tmp/switchy-async-owned-verification
cargo fmt -p switchy_async -- --check
for f in owned-driver owned-driver,tokio owned-driver,simulator; do
  cargo test --offline --locked -p switchy_async --no-default-features --features "$f" --test owned_scheduler_contract --test root_driver_contract
done
for i in 1 2 3; do
  cargo test --offline --locked -p switchy_async --no-default-features --features owned-driver --test owned_scheduler_contract -- deterministic_ready_trace --exact
done
cargo clippy --offline --locked -p switchy_async --no-default-features --features owned-driver,tokio --lib --test owned_scheduler_contract -- -D warnings
```

Final log `/tmp/switchy-owned-sched-shutdown.log`: pass (scheduler/root: 2/1, 3/2, 4/2; three fresh-process exact tests; fmt and strict Clippy). Simulator-only configuration still warns about unused `spurious_point`/`SpuriousPoint`. Initial validation timed out on the shared Cargo cache lock; used a private local registry cache copy plus the locked bmux checkout/DB, no fetch or lockfile change. Initial lint findings (mutex guard scope and test documentation) and an unused test import were corrected. No global build/test or consumer suite ran.

Still gated: production consumer parity, spawning from a running task, multi-thread scheduling, cross-thread handle control, shutdown concurrent with polling, native IO/timer registration, broad seed/fault and performance coverage. The task slice does not replace Tokio or simulator runtime entry.

`owned-driver` exposes `switchy_async::owned::block_on(future)`. With default features disabled it needs neither async backend. Use it for a single root whose wake source makes progress independently of the calling thread. No existing builder, macro, task, blocking bridge or backend selection calls it. No workspace manifest or lockfile integration is needed.

## Contract boundary

| Behavior | Evidence | Still serving it |
| --- | --- | --- |
| Wake during poll, duplicate wake, external-thread wake; preserve local non-Send state | `root_driver_contract`: identical future under futures' executor and the owned driver | Owned only on explicit call |
| Join returns result, detach on handle drop, abort pending work releases captures, abort after completion preserves result, join-set cleanup | `application_runtime_contracts`, `join_set_contract` | Existing runtime |
| Ready work at expired deadline, pending work dropped on timeout, interval deadlines/reset | `application_runtime_contracts` | Existing time driver |
| Progress under bounded backpressure, biased shutdown selection; seeded unbiased selection and simulator waiter order | `application_runtime_contracts`, `select_contract`, `sync_determinism` | Existing scheduler; not a universal FIFO or bounded-latency guarantee |
| Pending read, external writer, partial write, buffered byte then EOF | `root_driver_contract` with `io` and a backend: Tokio in-memory duplex, on both root drivers | Existing Tokio IO types; **not** OS readiness evidence |

## Residual dependencies and gates

- `src/lib.rs` re-exports `futures` for either backend. Simulator macro expansion uses its fuse/select/join combinators. Simulator runtime join delivery and cancellation use futures oneshots and abort registrations; simulator utility/time/future code, barrier and test probes also use futures. The flume MPMC wrappers and Tokio utilities still use futures waker/combinator APIs. Adding the owned feature to an existing backend removes none of these dependencies.
- `src/tokio/{runtime,task,time,util}.rs` retain Tokio runtime entry, scheduling, local/blocking tasks, joins, timers and tokio-util integration. Sync includes Tokio channels/locks and flume; `bounded.rs` exposes Tokio error types. Simulator scheduling/clock and fault injection stay separate from this blocking driver. Simulator features still select Tokio, including via macros.
- Both `io.rs` modules expose Tokio traits, `ReadBuf` and extension types. Tokio runtime enables configured drivers; `fs` and `net` forward dependency features; Tokio process types operate on host processes/pipes, simulator process uses mocks with Tokio read traits. No change to native socket, file, process, signal, platform poller or timer paths.
- Before broader routing: prove task lifecycle, cancellation races, nested entry, fairness under load, timer registration/expiry, native readiness and shutdown for the candidate. A root waiter cannot drive tasks or descriptors on the same blocked thread. Downstream API and platform parity, performance and shared-workspace integration are unverified. Do not switch defaults on this evidence.

## Local verification (57cf1627f42d)

All passing cargo commands below ran inside `nix develop --offline --no-write-lock-file -c sh` from the checkout flake, with `export CARGO_TARGET_DIR=/tmp/switchy-async-owned-verification` (rustc 1.99.0). Commands shown as shell loops to identify exact features/targets. Baseline ran in the checkout:

```sh
for b in tokio simulator; do
  cargo test --locked --offline -p switchy_async --no-default-features --features "$b,macros,sync,time,io" --test application_runtime_contracts --test join_set_contract --test select_contract || exit
done
```

Passed: 44/9/2 on Tokio, 39/11/3 on simulator. Pre-flake invocation of the Tokio application test with system-profile Nix cargo failed linking `-liconv`; setting SDKROOT/LIBRARY_PATH outside the flake did not fix linking. Final shared-checkout test with `--locked` refused a workspace lockfile update during concurrent unrelated manifest changes. Did not edit the lockfile. Isolated verification source prepared from the checkout:

```sh
mkdir -p /tmp/switchy-async-owned-source
git archive 57cf1627f42d | tar -x -C /tmp/switchy-async-owned-source
for f in Cargo.toml src/lib.rs src/owned.rs tests/root_driver_contract.rs; do
  cp "packages/switchy/async/$f" "/tmp/switchy-async-owned-source/packages/switchy/async/$f"
done
```

Inside the same flake environment:

```sh
set -e
cd /tmp/switchy-async-owned-source
export CARGO_TARGET_DIR=/tmp/switchy-async-owned-verification
cargo fmt -p switchy_async -- --check
cargo test --locked --offline -p switchy_async --no-default-features --features owned-driver --test root_driver_contract
for b in tokio simulator; do
  cargo test --locked --offline -p switchy_async --no-default-features --features "owned-driver,$b,macros,sync,time,io" --test application_runtime_contracts --test join_set_contract --test select_contract --test root_driver_contract
done
for f in owned-driver owned-driver,tokio,io; do
  cargo clippy --offline --locked -p switchy_async --no-default-features --features "$f" --lib --test root_driver_contract -- -D warnings
done
for extra in "" ,owned-driver; do
  cargo test --locked --offline -p switchy_async --no-default-features --features "thread-simulator,sync,util$extra" --test select_contract --test sync_determinism
done
```

All passed: one standalone driver test; unchanged baseline counts plus two root tests on each backend; 4 select and 2 determinism tests in each seeded leg. Initial seeded run without `util` discovered zero determinism tests and is not counted. Earlier strict lint found missing panic documentation and test-only non-Send/clone-waker lints; final strict runs passed. Logs: `/tmp/switchy-async-nix-baseline.log`, `/tmp/switchy-async-owned-final2.log`, `/tmp/switchy-async-owned-fairness-final.log`.
