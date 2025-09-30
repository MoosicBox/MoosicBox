# Native Opus Decoder Implementation

## Mission
Implement a 100% safe, native Rust Opus decoder following RFC 6716, providing an alternative to libopus (C library) while maintaining full RFC compliance and memory safety.

## Goals
- Zero unsafe code (except safe wrappers for SIMD intrinsics if needed)
- RFC 6716 bit-exact compliance
- Production-ready audio quality
- API compatibility with audiopus (libopus bindings)
- Comprehensive test coverage (RFC test vectors + fuzzing)
- Feature-gated decoder modes (SILK/CELT/Hybrid)
- Backend selection via feature flags (native vs libopus)
- Zero-cost abstraction for backend selection

## Non-Goals (Initially)
- Encoder implementation (decoder-only focus)
- Matching libopus performance before correctness is proven
- Hardware-specific optimizations (portable first)
- Supporting non-standard sample rates or configurations

## Constraints
- NO COMPROMISES on RFC 6716 compliance
- Zero clippy warnings policy
- All business logic must have tests
- Document all DSP algorithms with RFC references
- Maintain MoosicBox code style (no comments, descriptive names)
- No timelines or effort estimates

## Architecture Overview

### Backend Selection (moosicbox_opus)

The `moosicbox_opus` crate provides backend selection via zero-cost re-exports:

**Features:**
- `native` (default) - Uses moosicbox_opus_native (pure Rust)
- `libopus` - Uses audiopus (C library via FFI)
- Stub backend - Compiles but panics if no backend enabled

**Priority:** If both `native` and `libopus` are enabled, `libopus` takes priority.

**Zero-cost re-exports:** Backend types are directly re-exported with no trait dispatch or wrapper overhead.

### Zero-Cost Abstraction Guarantee

The backend selection mechanism uses **direct re-exports** rather than trait dispatch:

- No trait objects
- No dynamic dispatch
- No wrapper structs
- No runtime overhead
- Compiler can inline across backend boundary

The selected backend's types are **directly re-exported** as the public API, ensuring zero abstraction cost.

### Decoder Modes (moosicbox_opus_native)

The native implementation supports feature-gated modes:

**Features:**
- `silk` (default) - Speech/narrowband decoder (RFC 4.2)
- `celt` (default) - Music/wideband decoder (RFC 4.3)
- `hybrid` (default) - Combined SILK+CELT mode (RFC 4.5)

**Binary size optimization:** Users can disable unused modes to reduce binary size.

### Package Structure

```
packages/
├── opus/                    # Backend selector & Symphonia integration
│   ├── src/
│   │   ├── lib.rs          # Zero-cost backend re-exports
│   │   └── stub_backend.rs # Fallback (panics at runtime)
│   └── Cargo.toml          # Features: native (default), libopus
│
└── opus_native/            # Pure Rust decoder implementation
    ├── src/
    │   ├── lib.rs          # Main API matching audiopus
    │   ├── range/          # Range decoder (RFC 4.1)
    │   ├── silk/           # SILK decoder (RFC 4.2) [feature: silk]
    │   ├── celt/           # CELT decoder (RFC 4.3) [feature: celt]
    │   ├── modes/          # Mode integration (RFC 4.5) [feature: hybrid]
    │   └── plc/            # Packet Loss Concealment (RFC 4.4)
    └── Cargo.toml          # Features: silk, celt, hybrid (all default)
```

## Dependencies Strategy

**moosicbox_opus dependencies:**
- bytes - byte buffer manipulation
- log - logging
- symphonia - codec trait integration
- thiserror - error handling
- moosicbox_opus_native (optional, default)
- audiopus (optional)

**moosicbox_opus_native dependencies:**
- bytes - byte buffer manipulation
- thiserror - error handling
- log - logging
- (ONLY workspace dependencies, add as needed)

## API Compatibility

The native implementation matches the audiopus API exactly:

```rust
pub enum Channels { Mono = 1, Stereo = 2 }
pub enum SampleRate { Hz8000, Hz12000, Hz16000, Hz24000, Hz48000 }

pub struct Decoder { /* ... */ }

impl Decoder {
    pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self, Error>;
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize, Error>;
    pub fn decode_float(&mut self, input: Option<&[u8]>, output: &mut [f32], fec: bool) -> Result<usize, Error>;
    pub fn reset_state(&mut self) -> Result<(), Error>;
}
```

**Constraint:** Type signatures must match audiopus exactly for zero-cost re-export to work.

## Success Criteria

- [ ] Decode all RFC 6716 test vectors correctly
- [ ] Pass Opus conformance test suite
- [ ] Integration with MoosicBox player works seamlessly
- [ ] API-compatible with audiopus (drop-in replacement)
- [ ] Zero clippy warnings with all feature combinations
- [ ] Comprehensive test coverage (unit + integration + conformance + fuzzing)
- [ ] Backend selection works correctly (native/libopus/stub)
- [ ] Zero runtime overhead from backend abstraction

## Dependencies on Current Work

**Reusable from spec/opus (moosicbox_opus package):**
- ✅ Packet parser (RFC Section 3) - Complete and tested
- ✅ TOC byte parsing
- ✅ Frame length decoding
- ✅ Frame packing (Code 0-3)
- ✅ Padding handling
- ✅ Error types
- ✅ Test infrastructure (48 tests)
- ✅ Symphonia integration layer

**New implementation in moosicbox_opus_native:**
- ❌ Range decoder (RFC Section 4.1)
- ❌ SILK decoder (RFC Section 4.2)
- ❌ CELT decoder (RFC Section 4.3)
- ❌ Packet Loss Concealment (RFC Section 4.4)
- ❌ Mode integration (RFC Section 4.5)

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- All technical decisions reference RFC 6716 for specification compliance
- NO COMPROMISES - halt on any deviation from spec
- Includes comprehensive test coverage for all business logic
- Tests must be written alongside implementation, not deferred
- Both success and failure paths must be tested
- Living documents that evolve during implementation
- After completing a checkbox, 'check' it and add details as PROOF

See `opus-native/plan.md` for implementation phases and current status.
