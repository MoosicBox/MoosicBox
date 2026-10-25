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
new WAV. CI workflow wiring remains coordinator-owned/proposed, not implemented.
The existing fixed oracle is not rebuilt, modified or used by the performance tool.
