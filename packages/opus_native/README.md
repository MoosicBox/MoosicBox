# moosicbox_opus_native

Pure Rust implementation of the Opus audio decoder (RFC 6716).

## Installation

```toml
[dependencies]
moosicbox_opus_native = "0.1.0"
```

Enable `resampling` only if you need SILK sample-rate conversion:

```toml
[dependencies]
moosicbox_opus_native = { version = "0.1.0", features = ["resampling"] }
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

- Packet loss (`None`) and `fec=true` currently return `Error::Unsupported` before changing decoder state or output; native PLC/FEC synthesis is not implemented.
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
* Independent persistent SILK checks cover 24 rate/channel/duration streams
  (192 packets) without alignment shifts or sample tolerances.
* FEC and packet loss (`None`) return `Error::Unsupported`; reference PLC/FEC
  synthesis is not implemented. Unverified hybrid mode/channel transitions and
  redundant transition synthesis also return explicit `Unsupported`. Redundancy
  rejection occurs after SILK state advancement; reset before retrying a stream.
* `reset_state` reconstructs fresh decoder state; `decode_float` converts integer
  decoder output rather than introducing floating-point codec synthesis.
* Integer range encoding is implemented in `range::RangeEncoder`; a native audio
  encoder is not yet available.
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
