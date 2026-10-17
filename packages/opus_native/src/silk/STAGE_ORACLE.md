# SILK MB stereo first-divergence oracle

`stage_oracle.py` compiles the adjacent C wrapper against an existing **independent
C libopus** static build. It verifies source revision
`34bba701ae97c913de719b1f7c10686f62cddb15` and tracked cleanliness, checks fixed
point configuration, and prints archive/oracle SHA-256. GNU link wrapping exposes
real `silk_decode_frame`, `silk_stereo_decode_pred`, and `silk_stereo_MS_to_LR`
outputs without changing vendored source or calling native Rust code.

Example from the repository root:

```sh
python3 packages/opus_native/src/silk/stage_oracle.py \
  --build target/debug/build/moosicbox_opus_native_libopus-f3d2ff998a04d366/out/build \
  --rate 12000 --channels 2 \
  packages/opus_native/src/silk/reference/mb_mixed_stereo.packet
```

Select the current fixed-point scalar C build explicitly if Cargo's build ID
changes. Additional packet arguments use the same persistent decoder. This small
oracle currently supports the pinned build without deep PLC/OSCE; it is not a
public codec API. Temporary compiler artifacts are automatically removed.

`reference/provenance.json` pins the observed C archive, oracle, packet and PCM
identities. Stage PCM is LE i16: 240 samples per decoded mid/side channel, 480
interleaved samples at LR and API stages. The packet is genuine code-3 padded CBR,
not a raw payload; stage tests explicitly account for its three framing bytes.

Finding: native mid **and** side samples match the reference before conversion.
Stereo conversion itself matches; caller interleaved from index 2 instead of 1,
skipping the first converted sample and emitting an unconverted tail sample.
The caller now returns the actual reference LR window. Exact stage tests cover
all samples (no SNR, delay search, minimum-length or tolerance acceptance).

Remaining coordinator-owned interface issue: the independent API output for this
12 kHz case equals LR delayed by **9 samples/channel**, including zero prefix.
This is evidence about this fixture, not permission to hardcode one rate's delay
for all streams. Preserve FIFO state and reference resampler behavior across
packets; do not trim expected PCM. Mono and other rates still need raw API tests.

Validation during A4: offline Cargo resolution was blocked by uncached
`symphonia-common` through workspace Symphonia 0.6.1. A direct `rustc --test`
compile of current native source with cached log/thiserror/test-log/test-case
artifacts, SILK-only features, passed all 198 SILK tests including both independent
stage tests. This is focused evidence, not broad Cargo acceptance or full-stream
parity. No vector, runner, core lib.rs or vendored file was edited.

## A5 integer codec output conversion

`silk::resampler::SilkResampler::new(input_rate, output_rate)` and
`process(&mut self, &[i16]) -> Result<Vec<i16>>` implement decoder-side reference
output conversion with persistent allpass/AR2/FIR and delay state. One mono
instance per output channel; deinterleave LR before processing and reinterleave
results. Always process same-rate output too: reference input delays are 4, 9,
12 samples for 8, 12, 16 kHz. No independent delay FIFO should be added on top.
Input blocks must be 1..120 whole milliseconds. Conversion returns exactly the
input duration at the output rate, includes raw initial delay, and preserves the
unemitted tail in state. Invalid blocks do not mutate state. Reset by constructing
a new instance only when the C reference would reset/reinitialize the converter.
Configuration transitions and changing API channel counts require caller policy.

All decoder-supported 8/12/16 → 8/12/16/24/48 kHz pairs use integer math only.
Coefficients are copied from the pinned reference. No native production FFI,
floating sample conversion or generic FFT is used in this component. The old
public f32 utility was migrated to Symphonia 0.6.1 for compatibility; it is not a
replacement for this reference path. Core routing is a coordinator/B edit.

`resampler_oracle.c` generates 3 persistent 20ms packets for each of 15 pairs.
`reference/resampler.pcm` (38,880 bytes) freezes every output sample;
`reference/resampler_provenance.json` records source, C archive, oracle and PCM
identities. Reproduce without changing expectations:

```sh
python3 packages/opus_native/src/silk/resampler_oracle.py \
  --build target/debug/build/moosicbox_opus_native_libopus-f3d2ff998a04d366/out/build
CARGO_BUILD_JOBS=2 cargo test --offline -p moosicbox_opus_native --lib \
  --no-default-features --features silk,resampling silk::
CARGO_BUILD_JOBS=2 cargo clippy --offline -p moosicbox_opus_native --lib \
  --no-default-features --features silk,resampling -- -D warnings
```

A5 results: 202 tests passed, including all 15×3 C trace comparisons, whole-block
versus 1ms partition equality on extrema, error/state preservation, and actual MB
stereo LR→raw API equality. C trace verification and strict Clippy passed. Default
CARGO_HOME was used (environment unset). These establish component parity, not
full native decoder stream parity or all configuration-transition behavior.

## A7 persistent voiced waveform repairs

The actual four-packet WB sequence is frozen under `reference/wb_sequence*`:
1280 decoded-channel samples and 3840 raw 48 kHz API samples. Packet 1 first
failed before the resampler at decoded sample 0 (-2664 vs -2730), despite exact
LPC matrices and gains. The old active voiced path discarded LTP history every
subframe. `voiced.rs` now retains PCM history, rewhitens with the selected LPC
matrix, applies reference inverse-gain/LTP scaling and Q15 five-tap prediction,
and preserves state across subframes/packets. Reference-approximate gain division
also scales LPC history; independent pitch coding uses packet boundaries.
Packet 3 exposed NLSF interpolation rounding instead of reference arithmetic
truncation; this is fixed. New state is reset with the decoder; invalid pitch or
voiced synthesis parameters return a codec error.

NB impulse had an independent entropy-order defect: extra pulse LSBs were read
bit-plane-major instead of sample-major. The loop nesting now matches C, and
all decoded samples match. Independent C parameter traces (gains, both LPC
matrices, pitch/taps, and source/archive/oracle identities) are recorded in the
new provenance JSON files. The expanded C oracle also emits excitation pulses.

Validation: 207 focused SILK tests pass, including every WB decoded sample,
every raw 48 kHz API sample, **public Decoder output** (960 samples per packet),
and the NB impulse decoded stage. Strict SILK-only library Clippy passes.
Unchanged runner inventory and all 43 fixture identities: 18→20 pass,
16→14 mismatch, 9 codec errors unchanged. All 19 SILK inventory cases pass;
this is not evidence that arbitrary SILK streams, channel/rate/duration changes,
PLC/FEC or the whole Opus corpus are correct. No frame-size reconfiguration API
was added in this increment. No runner/core/corpus changes were made by A7.

## A8 stereo-coded to mono API

Production method:

```rust,ignore
silk.decode_silk_frame_stereo_to_mono(&mut range, (mid_vad, side_vad), &mut mono)?;
```

It returns samples/channel at SILK's internal rate and consumes both channel
payloads. Run the mono result through **one persistent A5 integer resampler**,
including same-rate delay. Retain `begin_packet()` boundaries. Do not decode to
LR and average, do not apply MS→LR to this branch, and do not add another sample
shift. Reference dec_API.c's mono-output branch copies two mid history samples,
updates sMid from the final decoded samples, and emits the window starting at
index 1. It does not update MS→LR predictor or side conversion history. Side
synthesis/entropy and decoded stereo predictors are still consumed/preserved.
Existing stereo API and behavior remain unchanged through a shared decode body.

The pinned independent C public API generated `reference/mb_mixed.api{1,2}.pcm`
and `reference/wb_mixed.api{1,2}.pcm`. Each contains two repeated nonzero packets
on a persistent decoder. Separate C invocations reset between API channel counts;
provenance records exact commands, source/archive/oracle and fixture identities.
Tests run both channel modes at MB 12 kHz and WB 16 kHz, then repeat from freshly
reset state, comparing every raw sample and frame count after reference-compatible
resampling. No averaging, SNR, delay search or tolerance is used.

A8 validation: 208 focused SILK tests and strict library Clippy passed. API routing
in core lib.rs/output.rs remains B-owned. This increment does not implement API
channel-count transitions within an already-running decoder, duration changes,
PLC/FEC or arbitrary-stream acceptance.

## A10 channel histories and side restart (bounded increment)

Mid/mono and side now retain independent NB/MB and WB previous NLSFs and
first-frame-after-reset flags. Side decoding swaps in its bank and restores it
on either success or codec error, so mono helper API/tests remain compatible.
A skipped side frame no longer forces mid interpolation to current NLSF.

The reference mid-only -> side branch resets side PCM and LPC synthesis memory,
gain-index history (10), pitch/voicing context and side first-reset flag, retaining
NLSF, previous synthesis gain and MS/LR conversion history. Conditional coding
uses the mid frame index (including skipped side frames). A restarted side in an
already-started packet is independently coded without consuming LTP scale bits.
No public API changes, sample shifts, trimming or tolerance were added.

### Evidence and reproduction

`a10_before.jsonl` / `a10_after.jsonl` preserve timestamped source/executable
identities, per-stream packet hashes, raw PCM hashes/counts, mismatch counts and
first difference (C then native). All 24 packet-stream identities are unchanged
between runs. Rates 8/12/16 kHz, channels 1/2, durations 10/20/40/60 ms, eight
persistent packets each: **9/24 exact -> 15/24 exact**, **71,044 -> 18,381**
interleaved sample mismatches. All 192 frame counts are exact. Every MB/WB
stereo 20/40/60 ms stream is now exact. Before-driver copies preserve E5's sine
signal (not integer-generated); the exact encoded inputs are retained so libm
variation cannot silently change the regression. No native expectation exists.

`reference/a10/provenance.json` records packet/oracle SHA-256s and pinned archive
`4f2a11b2264e7172169207a29a35e624bda97fdfbd3e2cdb4026bff658b4d500`.
C-source revision/configuration and archive were checked by the existing pinned
reference audit. `matrix_generate.c` and `matrix_decode.c` are dev-only E5 driver
copies; `matrix_native.rs` is a standalone public-API diagnostic (not a Cargo
module). Build explicit binaries against that archive and the freshly built
native library, then run without `--create` to preserve stored packets:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked --offline -p moosicbox_opus_native --no-default-features --features silk,resampling
rustc --edition=2024 packages/opus_native/src/silk/matrix_native.rs --extern moosicbox_opus_native=target/debug/libmoosicbox_opus_native.rlib -L dependency=target/debug/deps -o /tmp/a10-native
python3 packages/opus_native/src/silk/matrix.py --native /tmp/a10-native --generate /tmp/a10-generate --reference /tmp/a10-cdecode --archive target/debug/build/moosicbox_opus_native_libopus-4d7c577870a89d38/out/build/libopus.a --streams packages/opus_native/src/silk/reference/a10
```

The separate `transition.packets` forces two coded channels; identical low-rate
LR for three packets, then changes within packet four. The C stage trace proves
mid-only frames followed by conditional=1 side restart **within a packet**, not
only across packet boundaries. All eight public C outputs match native exactly.
The regression test replays this and six MB/WB streams twice with fresh decoders,
checking every raw sample and count (112 packet decodes). Public C oracle blobs
contain u32le sample-count followed by full interleaved i16le PCM for each packet.

Validation: 210 focused SILK tests pass; strict library Clippy passes; owned Rust
formatting passes. Mandatory corpus diagnostic retains all 19 SILK cases and
unchanged corpus SHA-256 `2204e845c74304ba6cead4a19c5052094a0336ad9cc11c5777e714adb7fcfd44`.
Current overall diagnostic is 34 pass / 9 codec errors, including concurrent
CELT/core work; this is not a claim that all codecs or arbitrary streams pass.

### Remaining failures (not weakened or accepted)

NB mono 10/40/60 ms, NB stereo 10/20/40/60 ms, MB mono 60 ms and MB stereo 10 ms
still mismatch. NB mono 10 ms first divergence stays packet 5 / sample 6:
C -7389, native -7387. A separate full C stage trace is retained at
`reference/a10/8000-1-10.trace.jsonl`; arithmetic divergence is not yet localized.
WB stereo NLSF trace is also retained. Duration/configuration transitions, PLC,
FEC and general-stream correctness are still outside this bounded acceptance.

## A11 missing production NLSF stabilization

The nine A10 matrix failures are resolved by calling the existing
`stabilize_lsf` at the end of `reconstruct_lsf`, matching `silk_NLSF_decode.c`.
Stabilization occurs before NLSF2A **and before saving interpolation history**.
No LPC/inverse-gain arithmetic changes, output shifts, resampler changes or
expectation changes were needed. Public interfaces and Q formats are unchanged.

First divergence localization (zero-based packet indices): NB mono 10 ms,
packet 5, decoded core sample 1 (raw API sample 6 after existing delays). C's
reconstructed adjacent NLSFs `[1831,1809]` cross; C stabilizes to `[1819,1822]`.
The remaining eight NLSFs are unchanged. The previously uncalled native
stabilizer produces exactly those values. Before stabilization native LPC was
`[8176,-4635,445,-213,656,-1356,1506,-397,-346,76]`; C and corrected native use
`[8190,-4651,447,-214,661,-1369,1524,-402,-351,79]`. The first sample difference
was C -7389 / native -7387. The retained independent C trace now wraps
`silk_NLSF_stabilize` and records both vectors:
`reference/a10/8000-1-10.a11.trace.jsonl`. A focused regression asserts these
vectors and C LPC coefficients. Temporary native diagnostic instrumentation and
an exploratory inverse-gain implementation were removed; only the missing
production call remains.

### Final independent proof

`a11_after.jsonl` records final timestamp/source/driver/executable identities,
raw PCM hashes, sample counts and differences. `a11_proof.json` pins final source
and independent C output identities. Against the same archive
`4f2a11b2264e7172169207a29a35e624bda97fdfbd3e2cdb4026bff658b4d500` and **unchanged
A10 encoded streams and reference hashes**:

* **24/24 streams, 192/192 packets exact**, every raw interleaved sample/count.
* Mismatches: **18,381 -> 0**; no dropped/warmup/alignment samples.
* Every 8/12/16 kHz × mono/stereo × 10/20/40/60 ms combination passes.
* All 24 streams plus the dedicated within-packet side-restart stream are now
  checked in the persistent regression, then replayed on fresh decoders: 400
  exact packet decodes per test invocation. Expected blobs come only from C.
* **211 focused SILK tests pass**, including the new localization assertion.
* Strict library Clippy (`--locked --offline`, `silk,resampling`, `-D warnings`)
  and owned Rust formatting pass.
* Diagnostic still has **19/19 mandatory SILK passes**, unchanged corpus hash
  `2204e845c74304ba6cead4a19c5052094a0336ad9cc11c5777e714adb7fcfd44`.
  All 181 source-controlled frozen vector files were independently hash-checked
  unchanged. Overall diagnostic remains 34 pass / 9 codec errors, not acceptance.

Validation commands use `CARGO_BUILD_JOBS=2`:

```sh
cargo test --locked --offline -p moosicbox_opus_native --lib --no-default-features --features silk,resampling silk::
cargo clippy --locked --offline -p moosicbox_opus_native --lib --no-default-features --features silk,resampling -- -D warnings
cargo run --locked --offline -p moosicbox_opus_native_test_runner -- --diagnostic
```

The A10 matrix reproduction command works unchanged with `/tmp/a11-native` and
final source-built library. All new C expected files use the documented framed
count + complete raw PCM format. A10 proof/provenance remains historical; A11
proof separately identifies new expected blobs and stage wrapper changes.

This closes the nine reproduced A10 gaps, not general arbitrary-stream coverage,
PLC/FEC, dynamic duration/configuration transitions or hybrid SILK/CELT mixing.
No hybrid Q-format interface change was necessary for this correction; the
existing public frame output remains internal-rate i16 PCM before one persistent
API-rate resampler, not CELT Q15 input. A hybrid bridge would need separate
end-to-end evidence rather than reinterpreting these i16 samples as Q15.
