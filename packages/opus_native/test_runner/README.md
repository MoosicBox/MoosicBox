# Native Opus feedback runner

This unpublished package's default executable is an **exact, uncompensated**
decoder scoreboard. Legacy compensated/ignored tests were replaced with exact raw SILK checks.
Those assertions intentionally fail for real mismatches; full CELT/hybrid known
failures execute in the strict scoreboard, not ignored compensated tests.

```sh
CARGO_BUILD_JOBS=2 cargo run --locked --offline -p moosicbox_opus_native_test_runner -- > strict.jsonl
CARGO_BUILD_JOBS=2 cargo run --locked --offline -p moosicbox_opus_native_test_runner -- --diagnostic > baseline.jsonl
CARGO_BUILD_JOBS=2 cargo run --locked --offline -p moosicbox_opus_native_test_runner -- --case sequences/celt_fb_mono --baseline baseline.jsonl
```

Default strict mode exits nonzero for every non-pass status, including unsupported
features. Explicit `--diagnostic` permits codec failures, never harness failures;
its successful collection is not codec acceptance. Unknown options/IDs and broken
inventory/provenance exit 2. A case-level harness failure exits 1. JSONL is stdout;
Cargo logs and caught Rust panic messages are stderr. All selected cases execute,
including known failures. Debug/release overflow behavior can differ: use the same
profile when comparing baselines.

`--root DIR` selects a copy of the built-in corpus. `required.rs` freezes all 43
v1 IDs, kinds, and packet counts independently of the supplied inventory. Any
subset (even the sole passing case), deletion, reclassification, or count change
is a harness failure before case selection, without requiring a baseline. A new
corpus needs an explicit trusted contract revision; arbitrary alternate subsets
cannot claim acceptance. Missing/empty inventory or provenance fails. Every
entry is loaded explicitly; missing, empty, odd-byte, mislabeled or checksum-invalid
fixtures are harness failures. Root reference revision/generator are pinned;
vector API v2 validates raw metadata, TOC, duration, checksums and reference config.
Fixture loader panics become harness failures and cannot terminate collection.
Paths must be relative canonical case IDs. Unlisted files are not coverage.

Single cases start from a reset decoder. Sequence cases load `sequence.json` and
use **one persistent decoder** for every packet; packet zero is not counted again
as a separate compatibility case. Each packet compares exact interleaved sample
counts and PCM, with no delay search, prefix removal, or minimum-length success.
A PCM mismatch does not stop later packets. After a codec error/panic, subsequent
packets are explicitly not exercised (decoder state is no longer trustworthy);
later cases still run. An unsupported case is not exercised and cannot pass strict
mode. PCM mismatch count, first differing sample, unpaired samples, max common
sample error, and output SHA-256 are recorded per packet.

Each run records features, Git HEAD, executable SHA-256, runtime source-tree
SHA-256, and complete reference provenance. Runtime source identity is an observed
working-tree snapshot, **not** a claim that those files produced an already-built
executable. Fixture hashes include packets, PCM, metadata, and sequence manifests;
summary corpus hash also includes inventory/provenance and selected case IDs.
Scoreboard schema **v3** baselines require header identities/features/profile and
reference provenance, allowed statuses, consistent exercised/packet records,
exact-comparison diagnostics, PCM identities, complete scope and consistent summary
counts/acceptance flags. Old v1/v2 or forged/incomplete records are rejected, not
silently upgraded. Source changes are expected for a fix; feature/profile/reference
provenance/comparison-contract changes instead produce `incomparable_configuration`.
Changed or unavailable fixtures are rejected before baseline comparison. Missing PCM evidence cannot yield `resolved`.
New and unselected baseline cases stay visible. Baselines remain unauthenticated
local evidence: schema checks cannot prove that a deliberately fabricated complete
record came from a genuine run. No known-failure allowlist changes expected PCM.

This is limited single-packet/persistent-sequence **decode** evidence. It does not
claim encode, PLC/FEC, cross-platform determinism, or end-to-end player acceptance.

Focused harness validation:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --offline -p moosicbox_opus_native_test_runner --bin moosicbox_opus_native_test_runner
CARGO_BUILD_JOBS=2 cargo clippy --locked --offline -p moosicbox_opus_native_test_runner --bin moosicbox_opus_native_test_runner --tests --no-deps -- -D warnings
```

## Schema 3 and process isolation

Version 3 explicitly rejects v1/v2 baselines. Case `actual_pcm_sha256` is now a
SHA-256 commitment over ordered packet index/status/actual-count/PCM-hash tuples,
not a hash of concatenated PCM bytes; packet hashes retain their raw PCM meaning.
The commitment is recomputed on load. Baselines must match the current fixture
identity and expected counts, and pass packets must match the reference PCM hash.
First-difference samples must agree with reference PCM and the maximum error
cannot be smaller than their absolute difference. Changed/unavailable fixture
corpora are errors, never silent progress. Baselines remain unauthenticated data;
these checks do not authenticate a deliberately fabricated matching transcript.

Each selected case runs in a private immutable copy of the current executable,
verified by SHA-256. On Linux the running inode is read via `/proc/self/exe`, so
concurrent Cargo replacement/unlink cannot redirect child execution. Snapshot
creation is exclusive, directory permissions are private (0700 on Unix), the file
is read/execute only (0500), and normal cleanup removes the file and directory.
The child inherits configuration, receives the canonical corpus root, verifies
its executable hash, and decodes directly (no recursive orchestration). No shell
or arbitrary executable option is supported. Cases have a fixed 10-second timeout;
timed-out children are killed/reaped. stdout/stderr are drained with bounded
retention. Exit code and Unix signal are included for crashes. `codec_timeout`
and `codec_crash` fail strict mode, but diagnostic collection may succeed; spawn,
identity, protocol and fixture failures remain harness failures in either mode.

Source identity includes workspace Cargo.toml/lock/config and pinned C sources in
addition to native/runner/vector/wrapper source. Runtime rustc/configuration hashes
are observations **at execution time**, not compiler/build provenance. No raw
configuration environment values or credentials are emitted. The executable hash
identifies the tested image; the runtime source snapshot does not prove its build.

The parent calls `frozen::verify_frozen_corpus` before starting any child and binds
`frozen::FROZEN_MANIFEST_SHA256` into the run header. Baseline loading requires that
same manifest identity. `--root` accepts only an unchanged complete frozen corpus;
a modified custom root is an explicit harness error, not an alternate oracle.
