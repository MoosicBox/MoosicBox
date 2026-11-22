# moosicbox_opus_native

Pure Rust integer-core Opus audio encoding and decoding (RFC 6716), with explicit supported configuration limits.

## Installation

```toml
[dependencies]
moosicbox_opus_native = "0.5.0"
```

Enable `resampling` only if you need SILK sample-rate conversion:

```toml
[dependencies]
moosicbox_opus_native = { version = "0.5.0", features = ["resampling"] }
```

## Usage

```rust
use moosicbox_opus_native::{Decoder, SampleRate, Channels};

// Create decoder
let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;

// Decode packet to i16 PCM
let mut output = vec![0i16; 960 * 2]; // 20ms @ 48kHz stereo
let samples = decoder.decode(Some(&packet), &mut output, false)?;
```

Critical public API notes:

- `decode_loss(output, samples_per_channel)` explicitly selects loss duration. CELT, SILK and hybrid concealment, burst recovery and reset have qualified fixed-point C coverage. `decode(None, ...)` uses the previous packet duration; explicit fresh-state loss produces silence.
- `fec=true` supports SILK redundancy and missing-LBRR concealment with bounded fixed-point C coverage, including hybrid-to-CELT transitions. Passing these cases does not establish universal conformance.
- SILK delay reporting: `decoder.algorithmic_delay_samples()` reports the integer resampler's included delay (4/9/12 input-rate samples, converted to the output rate), not wrapper history or container pre-skip.
- `decode_float` converts integer PCM output; `reset_state` reconstructs fresh decoder state.

## Features

- `silk` (default): SILK decoder for speech/narrowband content
- `celt` (default): CELT decoder for music/wideband content
- `hybrid` (default): Combined SILK+CELT decoder mode
- `resampling`: SILK resampling support (enables `silk`; requires moosicbox_resampler and symphonia)

## Reference Implementation

This implementation is based on the official Opus reference implementation from Xiph.Org Foundation:

- **Official Repository**: https://gitlab.xiph.org/xiph/opus
- **Commit**: `34bba701ae97c913de719b1f7c10686f62cddb15`
- **License**: BSD 3-Clause
- **Specification**: RFC 6716 - https://datatracker.ietf.org/doc/html/rfc6716

All algorithms are independently implemented based on the RFC 6716 specification, with verification against the reference implementation. See [REFERENCES.md](REFERENCES.md) for detailed source mapping.

## Implementation Status

This is an experimental encoder and decoder, not a production-complete native Opus codec.
Passing unit tests do not establish end-to-end waveform correctness.

* The frozen reference corpus passes exact raw PCM comparisons for all 43 cases
  (19 SILK, 15 CELT, 9 hybrid), including persistent nonzero sequences. These
  bounded results do not establish correctness for arbitrary Opus streams.
* Decoder coverage includes persistent channel/bandwidth changes, SILK FEC,
  short losses, recovery, and nonzero Q8 gain. Current exact comparison tests
  cover the previously failing hybrid-to-CELT FEC/PLC transition and assert actual
  TOCs; mode requests alone do not establish that a transition was exercised.
  This coverage remains bounded, including selected long-duration LBRR cases.
* `reset_state` reconstructs decoder state while preserving configured Q8 gain.
  `decode_float` converts integer decoder output. The Symphonia adapter preserves
  its existing normalized-float gain contract separately from integer codec gain.
* Native audio encoding is available through `Encoder::new` (CELT), `new_silk`,
  `new_hybrid`, and additive `new_auto` / frame-boundary `set_control` APIs.
  Existing constructors and configuration literals retain their meaning.
  * CELT: 8/12/16/24/48 kHz, mono/stereo, 2.5/5/10/20 ms. The 2.5-ms minimum is
    64 kbit/s mono or 128 kbit/s stereo; 5-ms minimum remains 64 kbit/s;
    10/20-ms minimum is 32 kbit/s. Maximum is 256 kbit/s. Low-rate stereo
    transient quality remains under investigation, even when packets decode correctly.
  * SILK: 8/12/16/24/48 kHz, mono/stereo, 10/20/40/60 ms; public bounds remain
    48–128 kbit/s mono or 96–128 kbit/s stereo until lower-rate qualification closes.
  * Hybrid: 24/48 kHz, mono/stereo, 10/20 ms; 128–256 kbit/s generally,
    with 32–256 kbit/s supported for 48 kHz stereo. Practical 32/64 kbit/s
    endpoints and 40/48/96 kbit/s intermediate rates have independent C
    waveform and synthetic/real-media quality coverage.
  * Auto: 48 kHz, 10/20 ms, fixed 384-sample lookahead; supports Fullband
    CELT/hybrid and Wideband SILK controls with qualified transition scheduling.
    Already-active stereo Hybrid supports steady 32–128 kbit/s controls;
    low-budget mode entry/departure remains rejected. Initial low-rate Auto
    selects CELT. This is not yet a general low-bitrate adaptation policy.
  * Query `lookahead()` and consume every packet returned by `finish()`.
    Voiced SILK and bounded rate-pressure quantization are implemented; real-media
    quality, all rate-control corners and broad perceptual acceptance remain unfinished.
* `multistream` composes validated mono/stereo streams with explicit channel maps,
  self-delimited framing, shared timing, reset and gain. Framing/routing
  interoperability passes; expanded exact waveform checks remain separate acceptance gates.
* `moosicbox_opus` offers explicit native selection, with libopus retained as its
  compatibility default. Native selection never silently falls back to C.
  Container logic belongs to `moosicbox_ogg_opus`, not the codec.
* Public product tests cover chained logical streams, 3/6/8-channel routing,
  arbitrary PCM chunks, terminal output errors, seeking, and exact 2457-sample
  trimming. Hardware playback and cross-platform execution are not established by these tests.

## Qualification through ordinary Cargo tests

`cargo test -p moosicbox_opus_native` runs the native qualification tests without
Python, subprocess orchestration, a C compiler, downloads, audio devices, or a
separate qualification workflow. The existing build-and-test pipeline selects
Ubuntu, macOS and Windows through this package's `clippier.toml`; that selection
is intended coverage, not evidence those runners have executed successfully.
Codec tests have no OS-specific expected values or OS exclusions. Feature gates
only reflect which codec implementations are enabled.

* `qualification_frozen` compares all 43 existing pinned fixed-point C cases
  (58 packets, 39,040 interleaved samples) to exact checked-in raw PCM, including
  persistent decoder sequences. No compensation or sample tolerance is applied.
* `qualification_regression` reuses the optional probe's integer-origin signal
  and malformed corpus, asserting common checked-in packet/PCM fingerprints for
  all five rates and both channel layouts, SILK/hybrid, and a two-minute CELT
  stream. Length-delimited packets and little-endian PCM are platform-neutral.
  These fingerprints are **native regression baselines captured on Linux x86_64**,
  not an independent C oracle, collision-resistant attestation, or perceptual
  quality proof. The native baseline update was explicitly approved on
  2026-10-24 after independent pinned-C validation and a source-stable repeat
  matched all 55 recorded files across 13 streams byte-for-byte. The reviewed
  normal frozen and regression suites passed locally on Linux x86_64; intended
  macOS/Windows coverage remains unverified. Frozen C expectations are unchanged.
  Tests never regenerate expectations. The optional probe prints read-only diagnostic
  fingerprints for deliberately requested capture/review after source settlement.
  Differences must be investigated, not automatically blessed.
* `qualification_completion` exercises deterministic malformed packets, output
  canaries, loss duration, reset and reconstruction reproducibility.

For focused execution use
`cargo test -p moosicbox_opus_native --test qualification_frozen --test qualification_regression --test qualification_completion`.
The optional `qualification_probe` example remains a local timing/file diagnostic;
normal tests do not launch it or require a custom workflow or script. Live pinned
C comparisons remain explicit opt-in tests and are distinct from checked-in
regression coverage. Local Linux execution cannot establish macOS/Windows, ARM,
audio-hardware, general Opus conformance or broad speech/music quality acceptance.

Use the [unpublished feedback runner](test_runner/README.md) for the current
mode-validated corpus, exact raw PCM comparisons, persistent sequences, and
reproducible failures. Its strict command returns nonzero while parity gaps
remain. Diagnostic collection success is not codec acceptance.

The documents under `spec/opus-native/` and `spec/opus/` are historical research.
Their phase-completion and production-readiness claims are not current evidence
or required architecture. Correctness is established by reproducible tests,
independent reference comparisons, and actual MoosicBox integration behavior.

## Acknowledgments

This implementation references the official Opus codec from Xiph.Org Foundation:

- Repository: https://gitlab.xiph.org/xiph/opus
- License: BSD 3-Clause
- Copyright: Xiph.Org Foundation

All algorithms were independently implemented based on RFC 6716 specification, with verification against the reference implementation for bit-exactness.

## License

See workspace LICENSE file.
