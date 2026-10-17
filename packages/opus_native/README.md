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
- `fec=true` supports verified complete-channel 20-ms SILK LBRR and verified missing-LBRR PLC fallback. Partial-channel LBRR, 40/60-ms SILK FEC, cross-mode FEC and some channel migrations remain explicitly unsupported.
- SILK delay reporting: `decoder.algorithmic_delay_samples()` reports the integer resampler's included delay (4/9/12 input-rate samples, converted to the output rate), not wrapper history or container pre-skip.
- `decode_float` converts integer PCM output; `reset_state` reconstructs fresh decoder state.

## Features

- `silk` (default): SILK decoder for speech/narrowband content
- `celt` (default): CELT decoder for music/wideband content
- `hybrid` (default): Combined SILK+CELT decoder mode
- `resampling`: SILK resampling support (requires moosicbox_resampler and symphonia)

## Reference Implementation

This implementation is based on the official Opus reference implementation from Xiph.Org Foundation:

- **Official Repository**: https://gitlab.xiph.org/xiph/opus
- **Commit**: `34bba701ae97c913de719b1f7c10686f62cddb15`
- **License**: BSD 3-Clause
- **Specification**: RFC 6716 - https://datatracker.ietf.org/doc/html/rfc6716

All algorithms are independently implemented based on the RFC 6716 specification, with verification against the reference implementation. See [REFERENCES.md](REFERENCES.md) for detailed source mapping.

## Implementation Status

This is an experimental decoder, not a production-complete native Opus codec.
Passing unit tests do not establish end-to-end waveform correctness.

* The frozen reference corpus passes exact raw PCM comparisons for all 43 cases
  (19 SILK, 15 CELT, 9 hybrid), including persistent nonzero sequences. These
  bounded results do not establish correctness for arbitrary Opus streams.
* Persistent SILK bandwidth/channel/duration changes, cooperative codec-mode transitions, abrupt nonredundant transitions, and loss/recovery have additional exact public-C tests. These bounded matrices are not universal conformance proof.
* Remaining guards include partial-channel LBRR, 40/60-ms SILK FEC, cross-mode FEC, FEC stereo-to-mono resampler migration, and CELT/hybrid coded-channel changes. Unsupported loss-duration combinations are rejected explicitly.
* `reset_state` reconstructs fresh decoder state; `decode_float` converts integer
  decoder output rather than introducing floating-point codec synthesis.
* Native audio encoding is available through `Encoder::new` (CELT), `new_silk` and `new_hybrid`, with integer analysis/quantization and explicit constructor limits. No automatic mode adaptation is implied.
  * CELT: 8/12/16/24/48 kHz, mono/stereo, 2.5/5/10/20 ms; minimum 128/64/32/32 kbit/s respectively, maximum 256 kbit/s.
  * SILK: 8/12/16 kHz, mono/stereo, 10/20 ms; 48–128 kbit/s mono or 96–128 kbit/s stereo.
  * Hybrid: 48 kHz, mono/stereo, 10/20 ms; 128–256 kbit/s.
  * Query `lookahead()` and consume every packet returned by `finish()`. Low-bitrate SILK noise shaping, voiced/LTP optimization and automatic adaptation remain unfinished; synthetic-signal quality checks do not establish general speech/music quality.
* `moosicbox_opus` offers an explicit native backend, with libopus retained as its
  default. Native selection never silently falls back to the C implementation.

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
