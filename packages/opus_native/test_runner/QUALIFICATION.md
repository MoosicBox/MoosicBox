# Reusable qualification invocation

This is explicit opt-in native-versus-pinned-C qualification, not a production
C dependency. Normal Cargo qualification tests retain the checked-in fixed-C
fixtures and run without Python, downloads, C builds or a media cache. Never
regenerate those expectations to make a run pass.

## Inputs and license review

`fixtures/qualification_inputs.json` pins the reference revision, whole-file WAV
SHA-256, size, audio format, source, attribution, usage/license URL and transform.
Provision those exact reviewed files in any caller-selected directory. Nothing
in the runner depends on `/tmp`, searches caches, downloads media, or silently
replaces a missing input. Speech is subject to its source site's conditions;
no redistribution permission or CC license is inferred. Music is Kevin MacLeod's
“Carefree,” CC BY 4.0, MP3-derived. Retain attribution and license metadata when
provisioning it. The historical transformed WAV hash includes its header; a
fresh FFmpeg conversion is not assumed byte-identical. Missing/mismatched media
fails before codec launch. Provisioning remains an explicit job-owner action.

## Coherent build evidence

After source settlement, capture `qualification_matrix.native_source_identity()`
before and after the actual native build, and SHA-256 of the tested images. The
identity includes native Rust source, adapter source and Cargo manifests/lock.
Do not generate a build receipt retroactively for an unrelated executable.

The caller supplies a JSON receipt with these required fields:

```json
{
  "schema": "moosicbox.opus.qualification-build.v1",
  "source_before_sha256": "<source identity before build>",
  "source_after_sha256": "<same identity after build>",
  "native_sha256": "<tested native executable SHA-256>",
  "reference_sha256": "<tested reference executable SHA-256>",
  "command": "<exact actual build command>",
  "profile": "release"
}
```

This is local build-owner evidence, not signed provenance. The matrix verifies
both source snapshots against current source and both images against their bytes.
Reference startup additionally verifies its pinned revision, cleanliness, fixed
point, disabled float API and disabled intrinsics/RES24/QEXT. Native source or
image mutation during execution prevents acceptance. Keep the reference Cargo
artifacts available for its own startup archive/config verification.

## Commands (job owner allocates the Cargo slot)

```sh
CARGO_BUILD_JOBS=2 cargo build --locked --offline --release \
  -p moosicbox_opus_native_test_runner --example native_oracle
CARGO_BUILD_JOBS=2 cargo build --locked --offline --release \
  -p moosicbox_opus_native_libopus --example reference_oracle
python3 -B -m unittest discover -s packages/opus_native/test_runner/src \
  -p qualification_inputs_tests.py
python3 -B packages/opus_native/test_runner/src/qualification_matrix.py \
  --native "$NATIVE_IMAGE" --reference "$REFERENCE_IMAGE" \
  --media-directory "$REVIEWED_MEDIA_DIRECTORY" \
  --build-receipt "$COHERENT_BUILD_RECEIPT" \
  --frames 12 --long-frames 6000 --adversarial-cases 128 \
  --output "$QUALIFICATION_REPORT"
```

Use explicit Cargo artifact paths (or Cargo JSON artifact records), not newest
cache discovery. No automatic reference fallback exists. Strict media coverage
includes Auto and explicit modes, >3-dB waveform SNR and <=2-LSB native/C decode
parity. Unsupported requested configurations still fail; a diagnostic collection
is not acceptance. `--diagnostic` permits legacy individual WAV paths but cannot
supply strict build/media evidence. The optional transient WAV remains diagnostic,
not a substitute for speech and music. Ordinary `qualification_completion` adds
<=1-LSB encoded-silence coverage with feature-aware constructors.

## Platform and performance evidence

Each report records actual OS/release/architecture/Python and total wall time.
That time includes process setup, JSON IPC and C: **not** a native throughput
benchmark. Codec throughput and memory budget are explicitly unverified. Record
release-profile per-frame encode/decode latency, real-time factor, peak RSS,
CPU/model, toolchain and source/image identities separately before making a
performance claim. Shared debug-worker timing is not a release budget.

Ubuntu/macOS/Windows selection in `clippier.toml` is intended coverage, not proof
those runners executed. Local Linux reports make no other-OS, ARM, hardware,
perceptual or general Opus-conformance claim.

Proposed CI wiring (coordinator-owned, not implemented here): retain ordinary
feature/platform Cargo jobs with fixed fixtures; add an explicitly opted-in job
that provisions reviewed media, builds both release images with coherent receipt,
runs harness tests and strict matrix, and uploads JSON/provenance even on failure.
Keep it separate from normal tests' C-free contract. Do not add a passing skip for
missing media or unexecuted platforms. Preserve high-rate cases and strict
2457-sample integration duration checks in product jobs.

## Offline release performance runner

`examples/qualification_performance.rs` measures codec calls in-process, without
C, subprocess/JSON work inside the timers, media files, network or floating-point
signal synthesis. PCM is exact integer-origin LCG data (seed 2457, signed i16
scaled by the binary factor 1/131072). It is a reproducible workload, **not**
speech/music quality evidence. Ten warm-up frames precede measured frames.
Standard 20-ms cases are CELT mono/stereo at 128k, CELT stereo at 256k, SILK
16-kHz mono at 128k, hybrid stereo at 128k and Auto stereo at 128k. Unsupported
features fail explicitly; use the default/full-feature release image for all six.
Eight additional practical-rate CELT, SILK, hybrid and Auto cases exercise the
currently public supported envelope. The runner requires at least two repetitions;
these timing workloads do not qualify low-rate transitions or release guards.

The receipt helper captures source identities around the actual allocated build
and obtains the executable from Cargo's artifact record:

```sh
python3 -B packages/opus_native/test_runner/src/qualification_build.py \
  --kind performance --target-directory "$PERFORMANCE_TARGET" \
  --receipt "$PERFORMANCE_RECEIPT"
```

After a granted build slot, capture source identity around this actual build:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked --offline --release \
  -p moosicbox_opus_native_test_runner --example qualification_performance
```

Supply a separate local receipt (not a quality-matrix receipt):

```json
{
  "schema": "moosicbox.opus.performance-build.v1",
  "source_before_sha256": "<qualification_matrix.native_source_identity before build>",
  "source_after_sha256": "<same identity after build>",
  "executable_sha256": "<exact release image SHA-256>",
  "profile": "release",
  "command": "<exact actual build command>",
  "rustc_version": "<actual rustc -Vv output>"
}
```

```sh
python3 -B packages/opus_native/test_runner/src/qualification_performance.py \
  --executable "$PERFORMANCE_RELEASE_IMAGE" --build-receipt "$PERFORMANCE_RECEIPT" \
  --frames 500 --repetitions 2 --timeout-seconds 300 --output "$PERFORMANCE_REPORT"
```

Every case uses a fresh child. Report includes per-case encode/decode total,
median, p95 and max nanoseconds, frames/s and audio real-time multiple, actual
CPU/OS, source/image/toolchain identities and deterministic input/packet/PCM
SHA-256. Repeat hashes must agree. The executable rejects debug-assertions builds;
the receipt records release profile (a local observation, not signed attestation).
Linux/macOS `wait4` gives this exact child's peak RSS, normalized to bytes; it
includes the codec, timing arrays, executable/runtime and harness, not isolated
codec allocation. Other platforms report peak memory unavailable, never zero or
invented evidence. Whole-process wall time is separate from codec-only timings.
Timeouts, changed source/image, bad duration or nondeterminism fail and retain a
partial report. No hardware-independent performance budget is asserted. Other OS
execution and CI success remain unverified until actual runs occur.

## Explicit CI media provisioning implementation

The offline default copies only already-reviewed media into a new exclusive
destination and verifies both ends:

```sh
python3 -B packages/opus_native/test_runner/src/qualification_provision.py \
  --source-directory "$APPROVED_MEDIA_ARTIFACT" --destination "$NEW_MEDIA_DIRECTORY"
```

An explicitly opted-in CI job can instead fetch the manifest-pinned speech URL
and an operator-supplied HTTPS location of the exact reviewed music WAV:

```sh
python3 -B packages/opus_native/test_runner/src/qualification_provision.py \
  --download --music-wav-url "$APPROVED_PINNED_MUSIC_WAV_URL" \
  --destination "$NEW_MEDIA_DIRECTORY"
```

Whole-file sizes, SHA-256 and WAV formats are checked before publication. A
missing URL, missing artifact, wrong bytes, HTTP downgrade or existing destination
fails, not skips. No download or upload was performed to implement this tool.
The derived music WAV has no claimed public hosting URL; an approved artifact
location is still required. Do not substitute the MP3 or silently transcode a
new WAV. The dedicated `.github/workflows/opus-native-qualification.yml` and
`src/qualification_ci.py` now provide fail-closed platform execution and artifact
comparison. Workflow implementation is not evidence of a successful platform run:
remote execution still requires separate authorization and reviewed hosted media.
The existing fixed oracle is not rebuilt, modified or used by the performance tool.

## Intended-support ledger and acceptance boundaries

Run the offline inventory without codec launches:

```sh
python3 -B packages/opus_native/test_runner/src/qualification_matrix_ledger.py \
  --media-directory "$REVIEWED_MEDIA" --output "$SUPPORT_LEDGER"
```

It records 88 requested public roundtrip cells (five rates, two layouts,
mode-legal durations), 31 real-media public-constructor requests, 36 encoded
silence cells, the 14 default public-envelope performance cases and two explicitly
unsupported SILK performance candidates. The SILK internal matrix is a separate
192-cell inventory. Actual runner AST loop metadata and function hashes expose
coverage drift. The ledger is **not** a passing report: unsupported requests stay
failures; internal test-only construction is not public-constructor acceptance.
Frozen cases, gain/replay, decode, long-stream and adversarial requirements retain
runner metadata as well. The normal Cargo feature/platform and product duration
checks remain separate required evidence, not satisfied by inventory generation.

At inspection on 2026-10-25 the manifest-pinned speech/music files were available
and hash/format verified in the explicit local directory
`/tmp/opus-complete-oracle-20261020`. No pinned files were missing there; this is
not a durable CI location or permission to publish. Reviewed fullband speech and
additional rate/layout/level variants remain absent if broader public support is
claimed. An approved remotely provisioned exact music-WAV artifact location is
still needed for CI; never substitute a new transcode or arbitrary current media.

Performance now reports `measurement_complete` separately from
`performance_accepted`. Acceptance requires every default public case at least
twice, identical input/packet/PCM hashes, source/image stability, valid integer
latency statistics, and **both total encode and total decode <= audio duration**
(at least 1x real time). Missing cases, malformed/nonfinite timing, slower phases
or candidate-only coverage fail and return nonzero. Median/p95/max remain explicit
observations; no prior numerical headroom promise is inferred. Proposed future
headroom policy, requiring explicit approval: >=2x for each phase and p95 <= one
frame duration. That proposal is not enforced or claimed as existing acceptance.

Raw `wait4` peak RSS may retain a pre-exec launch footprint; identical RSS across
all cases can be dominated by that floor. It is the measured child high-water
mark, **not isolated codec allocation or a verified codec-memory budget**. Local
throughput acceptance says nothing about another CPU/OS, quality or CI success.
The historical 50-frame r3 report completed 28 measurements but its minimum
encode multiple was about 0.1223x: it is not real-time acceptance. A current-source
coherent release receipt and fresh run remain necessary after any codec changes.

## Exact 192-case SILK command and serial shards

The case product is speech/music x 8/12/16 kHz x mono/stereo x 10/20/40/60 ms x
16/32 kbit/s per channel x scales 1/0.01. Filters are exact strings such as
`speech/8000/1/10/16000/1`. This executes internal candidates with unchanged
thresholds; it does not open the public guard or establish perceptual certification.

```sh
python3 -B packages/opus_native/test_runner/src/qualification_silk.py \
  --executable "$PREBUILT_RELEASE_UNIT_TEST_IMAGE" --reference "$FIXED_ORACLE" \
  --media-directory "$REVIEWED_MEDIA" --output-directory "$NEW_EVIDENCE_DIR" \
  --timeout-seconds 300
```

The exact child command is
`$PREBUILT_RELEASE_UNIT_TEST_IMAGE encoder::silk::public::internal_practical_floor_media_matrix --exact --ignored --nocapture`,
with one generated `MOOSICBOX_OPUS_SILK_CASE` and reviewed media/reference env per
child. No Cargo or implicit image discovery occurs. Source/compiler hashes are
runtime observations, not retrospective build receipts.

For serial bounded shards, generate repeated `--cases FILTER` arguments from
`qualification_silk.cases()`, e.g. the 96 entries starting with `speech/`, then
invoke the same output directory with `--resume` for the 96 `music/` entries.
Do not run shards concurrently in one evidence directory. Focused shards return
nonzero until all 192 pass; do not suppress that into full acceptance. Resume
requires identical source/image/reference/media/tool/runner provenance and
revalidates each saved passing log hash, zero exit, exact single-test execution
and single internal case marker. Changed source needs a new evidence directory;
failures/timeouts retain logs and do not stop later cases. All 192 verified passes
are required for `complete=true`; that still means internal-case validation only.

## Local native-prefix scoring evidence

Production Hybrid prefix search now scores legal SILK candidates against an
unweighted native-clock delayed target using cloned incoming decoder state.
Mono scores legal trials; stereo preserves each raw-search baseline and performs
two native-score coordinate passes, without claiming global pair optimality.
Production selection does not depend on a scorer environment flag or the C oracle.

A frozen production image passed all 40 practical-layout cells without the scorer
opt-in flag; its 240 quality/error rows matched the preceding test-only version.
Replay checks included 560 actual following-Hybrid prefix identity checks.
Subsequent frame-local caching passed three cache-on/off packet/PCM/winner/state
comparisons and two formerly failing layout cells with unchanged gate metrics.
Cache keys losslessly represent configuration, original payload length and bytes
through the last nonzero byte; decoder input is not trimmed. The full40 run
predates caching and is not current-source full-matrix acceptance.

Acceptance remains incomplete: the production default unit run timed out at
200 seconds, and comprehensive public RD, product, public SILK, current-source
debug/release, platform and performance qualification remain pending. The
production 40-cell run took 1059.31 seconds; realtime performance is not proven.
Existing thresholds, frozen expectations, strict duration checks and public
constructor guards remain unchanged.
