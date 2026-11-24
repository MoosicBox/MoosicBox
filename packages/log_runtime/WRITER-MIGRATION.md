# Bounded writer: contract and consumer handoff

The oracle is `tracing-appender` 0.2.5 with its **default lossy** non-blocking builder, not its lossless builder. BMUX `prune-deps` at `a6adfab0` locks crates.io `moosicbox_log_runtime` 0.5.0; its cached `src/lib.rs` matched the pre-change workspace file byte for byte. Editing this workspace does not change that binary.

## Contract

- Each producer `write` (including `write_all`) submits one intact buffer, not one newline or one byte. Clones share 128,000 queue slots and a FIFO receiver. Under contention only queue order is defined. Full/disconnected writes return success for the whole buffer and increment a shared saturating drop count. No capacity wait or sink I/O on producer threads. This bounds the number of buffers, not individual buffer size.
- Producer `flush` is not a barrier. The worker uses `write_all` for short writes and flushes on empty batches and shutdown. Sink I/O errors do not propagate to producers. Keep the guard alive; dropping the last producer alone is not shutdown.
- Dropping the guard attempts to enqueue a FIFO stop for 100 ms; if admitted it waits up to 1 s for the batch flush, without an unconditional join. This is not `fsync` or a guarantee under a stalled/broken sink. Writes racing or following the stop need not be delivered. A timed-out stop admission leaves a live worker consuming while senders remain. The std queue has no stable timed send; only the guard retries at up to 1 ms intervals, rather than the incumbent channel's wakeup. Timer and concurrent-admission schedules are not identical. One intentional cleanup difference: if all senders are gone and a final flush persistently fails, the worker reports the final failure and exits rather than retrying forever with no possible future records.
- No changes to fmt layers, subscriber/filter precedence, stderr policy, target or ANSI flags. Exact files append. Daily opens eagerly, appends to `prefix.YYYY-MM-DD` (just the date for empty prefix), uses the worker's UTC day and creates parent directories. Rollover only advances; an open failure retains the old file until the next date. As before, initial daily-open failure panics, whereas exact-open failure returns `InitError::CreateLogFile`.

## BMUX migration (not applied)

After an authorized release, select the actual new registry version in BMUX's workspace dependency and refresh the lock. For local integration a **temporary** `[patch.crates-io]` entry for `moosicbox_log_runtime` pointing at this `packages/log_runtime` is needed instead. Do not ship a path override. No additional production feature needed: `bmux_cli` already requests `file`.

1. `packages/cli/src/runtime/bootstrap.rs` (diagnostic writer setup): replace `tracing_appender::non_blocking(writer)` with `moosicbox_log_runtime::writer::non_blocking(writer)`. Keep `with_writer(non_blocking_writer)`, the filter/layer stack and log bridge as is. Unified mode's `init::FileMode::Exact` requires no change.
2. `packages/cli/src/runtime/plugin_kernel.rs`: change only the field type to `_guard: moosicbox_log_runtime::writer::WorkerGuard`. Keep `_guard` **before** `handle: bmux_diagnostic_log::DiagnosticLogHandle`; queued writes must flush before manifest finalization in the normal case.
3. In `packages/diagnostic-log/src/lib.rs` change the constructor in `non_blocking_guard_flushes_before_manifest_finalization` exactly as above. Replace its Cargo **dev-dependency** on `tracing-appender` with `moosicbox_log_runtime = { workspace = true, features = ["init"] }`. Keep the test and its drop order, not just a check that a file exists.
4. Remove the CLI's direct appender dependency, then remove BMUX's workspace declaration if no other manifest inherits it. Recheck the resolved graph, not just the manifest: the inspected lock also lists `turso_sdk_kit` and `turso_sync_sdk_kit` edges. Don't remove audit entries while actually reachable. Do not move BMUX segmentation, redaction, manifest policy, or handle lifetime into this generic package.
5. Consumer integration gate: check `bmux_cli` under its real feature set and run **only** `cargo test -p bmux_diagnostic_log --lib non_blocking_guard_flushes_before_manifest_finalization` in BMUX's pinned Nix shell. Confirm exact file contents and nonzero finalized manifest byte count. Neither a release nor a BMUX build/test was performed here.

**MoosicBox integrator:** regenerate the root lock. The resolved log-runtime entry drops `tracing-appender`, adds `time 0.3.55`, `tempfile` and `tracing` (last two for tests); let Cargo prune only unreachable entries. Remove the unused root `tracing-appender = "0.2.5"` declaration after checking concurrent lanes. No new workspace dependency keys are needed.

## Local verification

Local macOS/arm64, base `57cf1627f42d`, pinned `rustc 1.99.0 (b940084d7 2026-09-28)`. Commands below are package scoped. To avoid touching the shared lock, `git archive HEAD | tar -x -C /tmp/log-runtime.QXkyDS` populated a private workspace. All Cargo tests/lints/tree commands below ran there via `nix develop --offline --no-write-lock-file --command sh -ec 'cd /tmp/log-runtime.QXkyDS; …'` from the source worktree. `cargo fmt -p moosicbox_log_runtime` ran in that Nix shell in the real checkout.

Oracle setup: use the base package manifest and `src/lib.rs`; copy the candidate integration tests, add workspace dev-dependencies `tempfile`, `tracing`, `time`, and substitute `tracing_appender::non_blocking` for the writer import in `tests/writer.rs`. Candidate setup: `cp -R packages/log_runtime/. /tmp/log-runtime.QXkyDS/packages/log_runtime`. No oracle dependency in the candidate manifest, including dev-dependencies.

```sh
# Oracle baseline (0 lib tests), then both implementations against the same contract:
cargo test --offline --locked -p moosicbox_log_runtime --features file --lib
cargo test --offline -p moosicbox_log_runtime --features file --test writer --test exact --test daily
# Candidate verification (also includes paths, doctests and rollover/error unit test):
cargo test --offline -p moosicbox_log_runtime --features file,layer-ext,log-compat,fail-on-warnings
cargo test --offline -p moosicbox_log_runtime --no-default-features
cargo test --offline -p moosicbox_log_runtime --all-features
cargo clippy --offline -p moosicbox_log_runtime --features file,layer-ext --all-targets -- -D warnings
cargo tree --offline -p moosicbox_log_runtime --features file -e normal
# Each implementation, five release samples per test:
cargo test --offline --release -p moosicbox_log_runtime --features file --test writer throughput -- --exact --ignored --nocapture
cargo test --offline --release -p moosicbox_log_runtime --features file --test writer append_throughput -- --exact --ignored --nocapture
```

Final fmt check and `git diff --check -- packages/log_runtime` passed. A final Cargo rerun timed out waiting for the shared package-cache lock (120 s); retried offline with `H=$(mktemp -d /tmp/logrt-cargo.XXXXXX); ln -s ~/.cargo/registry "$H/registry"; ln -s ~/.cargo/git "$H/git"` and `env CARGO_HOME="$H"` inside the same Nix shell. The final candidate differential suite, all-features test and scoped Clippy exited 0. No appender in the candidate package tree. Five shared contract cases passed on each: exact bytes/filter/append, daily naming/append, stalled-sink overload/FIFO/short writes, saturated shutdown admission timeout, admitted shutdown flush timeout. Initial oracle setup used BSD `sed -i ''` on GNU sed and failed to switch the import; corrected with `sed -i`. Initial candidate Clippy errors (deprecated atomic method, redundant annotation and visibility) were corrected and rerun.

100,000 × 128-byte writes, one producer, no competing benchmark jobs launched here. Median milliseconds (enqueue / enqueue through guard drop):

| Sink | Oracle | Owned |
| --- | ---: | ---: |
| `io::sink()` | 6.03 / 6.09 | 2.66 / 3.14 |
| Append file, 12,800,000 bytes verified each run | 1.94 / 737.78 | 1.59 / 471.55 |

Local smoke measurements, **not** a throughput guarantee. Repeated sink samples varied (an earlier candidate run measured 6.63 / 6.68 ms median). Not measured: BMUX end-to-end formatter/rotation cost, multiproducer tail latency, other platforms or storage. Source integration, registry selection, BMUX manifest test, and product performance remain separate gates. No global build/nextest, cluster tests, remote execution, commits or pushes.
