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

- Packet loss concealment entry point: pass `None` as input (`decoder.decode(None, &mut output, false)`) to decode a lost packet.
- SILK delay reporting: call `decoder.algorithmic_delay_samples()` to get the current SILK algorithmic delay included in output.
- `decode_float` and `reset_state` are public but not implemented yet (Phase 6) and currently panic if called.

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

- [x] **Phase 1**: Range Decoder (RFC Section 4.1)
- [x] **Phase 2**: SILK Decoder - Basic Framework (RFC Section 4.2)
- [x] **Phase 3**: SILK Decoder - LSF/LTP/Excitation (RFC Section 4.2)
- [x] **Phase 4**: CELT Decoder (RFC Section 4.3)
- [x] **Phase 5**: Mode Integration & Hybrid
- [ ] **Phase 6-10**: PLC, Backend Integration, Testing, Optimization, Documentation

See `../../spec/opus-native/plan.md` for detailed implementation roadmap.

## Acknowledgments

This implementation references the official Opus codec from Xiph.Org Foundation:

- Repository: https://gitlab.xiph.org/xiph/opus
- License: BSD 3-Clause
- Copyright: Xiph.Org Foundation

All algorithms were independently implemented based on RFC 6716 specification, with verification against the reference implementation for bit-exactness.

## License

See workspace LICENSE file.
