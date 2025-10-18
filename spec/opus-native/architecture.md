# Native Opus Decoder Architecture

## High-Level Overview

The native Opus decoder is a pure Rust implementation of RFC 6716 (Opus Audio Codec) designed as a drop-in replacement for libopus while maintaining full API compatibility, memory safety, and zero abstraction overhead.

## Component Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      moosicbox_opus                         │
│  (Backend Selector & Symphonia Integration)                 │
│                                                             │
│  Zero-Cost Re-exports (no trait dispatch, no wrappers)      │
│                                                             │
│  #[cfg(feature = "libopus")]                                │
│  pub use audiopus::*;              ─────────────────────────┼──► audiopus
│                                                             │    (C library)
│  #[cfg(feature = "native")]                                 │
│  pub use moosicbox_opus_native::*; ─────────────────────────┼──► moosicbox_opus
│                                                             │    _native
│  #[cfg(no backend)]                                         │    (Pure Rust)
│  pub use stub_backend::*;          ─────────────────────────┼──► stub_backend
│                                                             │    (panics)
└─────────────────────────────────────────────────────────────┘
                              │
                ┌─────────────┴─────────────┐
                │                           │
        ┌───────▼────────┐          ┌──────▼──────┐
        │ moosicbox_opus │          │  audiopus   │
        │    _native     │          │  (libopus)  │
        │                │          │             │
        │ ┌────────────┐ │          └─────────────┘
        │ │   Range    │ │
        │ │  Decoder   │ │
        │ └─────┬──────┘ │
        │       │        │
        │ ┌─────▼──────┐ │
        │ │   SILK     │ │  [feature: silk]
        │ │  Decoder   │ │
        │ └────────────┘ │
        │ ┌────────────┐ │
        │ │   CELT     │ │  [feature: celt]
        │ │  Decoder   │ │
        │ └────────────┘ │
        │ ┌────────────┐ │
        │ │   Mode     │ │  [feature: hybrid]
        │ │Integration │ │
        │ └────────────┘ │
        │ ┌────────────┐ │
        │ │    PLC     │ │
        │ └────────────┘ │
        └────────────────┘
```

## Backend Selection Strategy

### Zero-Cost Re-exports

Instead of using trait dispatch or wrapper structs, we use conditional compilation to directly re-export the chosen backend's types:

```rust
// moosicbox_opus/src/lib.rs

// Libopus backend (takes priority)
#[cfg(feature = "libopus")]
pub use audiopus::{Channels, SampleRate, Error};
#[cfg(feature = "libopus")]
pub use audiopus::coder::Decoder;

// Native backend (fallback)
#[cfg(all(feature = "native", not(feature = "libopus")))]
pub use moosicbox_opus_native::{Channels, SampleRate, Error, Decoder};

// Stub backend (no backend enabled)
#[cfg(not(any(feature = "native", feature = "libopus")))]
mod stub_backend;
#[cfg(not(any(feature = "native", feature = "libopus")))]
pub use stub_backend::{Channels, SampleRate, Error, Decoder};
```

**Performance characteristics:**

- ✅ Zero runtime overhead
- ✅ Perfect inlining across backend boundary
- ✅ No trait dispatch
- ✅ No wrapper struct indirection
- ✅ No type conversion costs

**Constraint:**

- ⚠️ Backend APIs must match exactly
- ⚠️ Type signatures must be identical

### Compile-Time Feature Resolution

**Priority order:** libopus > native > stub

| Features Enabled                           | Backend Used | Dependencies Compiled | Warning | Bloat |
| ------------------------------------------ | ------------ | --------------------- | ------- | ----- |
| (default)                                  | native       | native only           | None    | None  |
| `native`                                   | native       | native only           | None    | None  |
| `libopus`                                  | libopus      | both                  | Yes     | Minor |
| `--no-default-features --features libopus` | libopus      | libopus only          | None    | None  |
| `native,libopus`                           | libopus      | both                  | Yes     | Minor |
| `--no-default-features`                    | stub         | none                  | Yes     | None  |

### Stub Backend Behavior

The stub backend compiles successfully but fails at runtime:

```rust
// moosicbox_opus/src/stub_backend.rs

#[derive(Debug, Clone, Copy)]
pub enum Channels { Mono = 1, Stereo = 2 }

#[derive(Debug, Clone, Copy)]
pub enum SampleRate { Hz8000, Hz12000, Hz16000, Hz24000, Hz48000 }

#[derive(Debug)]
pub enum Error { NoBackend }

pub struct Decoder { _private: () }

impl Decoder {
    #[cold]
    #[inline(never)]
    pub fn new(_sample_rate: SampleRate, _channels: Channels) -> Result<Self, Error> {
        panic!("No Opus backend enabled! Enable 'native' or 'libopus' feature.")
    }

    #[cold]
    #[inline(never)]
    pub fn decode(&mut self, _: Option<&[u8]>, _: &mut [i16], _: bool) -> Result<usize, Error> {
        unreachable!("Decoder construction should have panicked")
    }

    #[cold]
    #[inline(never)]
    pub fn decode_float(&mut self, _: Option<&[u8]>, _: &mut [f32], _: bool) -> Result<usize, Error> {
        unreachable!("Decoder construction should have panicked")
    }

    #[cold]
    #[inline(never)]
    pub fn reset_state(&mut self) -> Result<(), Error> {
        unreachable!("Decoder construction should have panicked")
    }
}
```

**Purpose:**

- Allows crate to compile in all scenarios
- Provides clear runtime error if misconfigured
- Build warnings alert developer at compile time
- `#[cold]` and `#[inline(never)]` minimize binary size

## Native Decoder Architecture (moosicbox_opus_native)

### Component Breakdown

**1. Range Decoder (RFC 4.1)**

- Entropy decoder using range coding
- Provides symbol extraction from compressed bitstream
- Used by both SILK and CELT decoders

**Key components:**

- `RangeDecoder` state machine
- Symbol decoding functions
- Raw bit extraction
- Uniformly distributed integer decoding

**2. SILK Decoder (RFC 4.2) [feature: silk]**

- Linear prediction based decoder for speech/narrowband
- Handles 8/12/16/24 kHz sample rates

**Key components:**

- `SilkDecoder` state machine
- LP layer organization
- LSF/LPC coefficient decoding
- LTP (pitch prediction) synthesis
- Excitation signal reconstruction
- Resampling to output rate

**3. CELT Decoder (RFC 4.3) [feature: celt]**

- MDCT-based decoder for music/wideband
- Handles 16/24/48 kHz sample rates

**Key components:**

- `CeltDecoder` state machine
- Energy envelope decoding
- Dynamic bit allocation
- PVQ (Pyramid Vector Quantization) shape decoding
- Inverse MDCT
- Post-filtering

**4. Mode Integration (RFC 4.5) [feature: hybrid]**

- Switches between SILK/CELT/Hybrid based on TOC byte
- Manages configuration transitions
- Handles redundancy

**Key components:**

- Mode detection from TOC
- State management
- Sample rate conversion
- Redundancy handling

**5. Packet Loss Concealment (RFC 4.4)**

- Handles missing packets gracefully
- Clock drift compensation

### Data Flow

```
Opus Packet (bytes)
      │
      ▼
┌─────────────┐
│   Packet    │  (Already implemented in moosicbox_opus)
│   Parser    │  - TOC byte
│  (RFC 3)    │  - Frame packing
└─────┬───────┘  - Padding
      │
      ▼
┌─────────────┐
│    Mode     │
│  Detection  │  Based on TOC config
└─────┬───────┘
      │
      ├─────────────┬─────────────┐
      ▼             ▼             ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│   SILK   │  │   CELT   │  │  Hybrid  │
│   Mode   │  │   Mode   │  │  (both)  │
└────┬─────┘  └────┬─────┘  └────┬─────┘
     │             │             │
     │  ┌──────────▼──────────┐  │
     └─►│   Range Decoder     │◄─┘
        │    (RFC 4.1)        │
        └──────────┬──────────┘
                   │
        ┌──────────┼──────────┐
        ▼                     ▼
  ┌───────────┐         ┌───────────┐
  │   SILK    │         │   CELT    │
  │  Decoder  │         │  Decoder  │
  │ (RFC 4.2) │         │ (RFC 4.3) │
  └─────┬─────┘         └─────┬─────┘
        │                     │
        │  ┌──────────────┐   │
        └─►│    Hybrid    │◄──┘
           │ Combination  │
           └──────┬───────┘
                  │
                  ▼
           ┌─────────────┐
           │   Output    │
           │ PCM Samples │
           └─────────────┘
```

### State Management

Each decoder maintains state across frames:

**SILK State:**

- Previous frame LPC coefficients
- Pitch lag history
- Gain smoothing state
- Resampler state

**CELT State:**

- Previous MDCT coefficients (overlap-add)
- Energy envelope history
- Post-filter state
- De-emphasis filter state

**Mode Switching State:**

- Previous mode configuration
- Transition smoothing buffers
- Redundancy frame storage

### Memory Management

**Zero-copy strategy:**

- Use `Bytes` crate for packet data
- Pass slices where possible
- Pre-allocate output buffers

**Buffer reuse:**

- Decoder maintains internal buffers
- Resize dynamically as needed
- Clear/reset for state reset operations

## Feature Flag Design

### moosicbox_opus_native Features

```toml
[features]
default = ["silk", "celt", "hybrid"]

silk = []           # SILK decoder (speech/narrowband)
celt = []           # CELT decoder (music/wideband)
hybrid = ["silk", "celt"]  # Combined mode
```

### Conditional Compilation

**Module structure:**

```rust
// moosicbox_opus_native/src/lib.rs

mod range;  // Always included (core dependency)

#[cfg(feature = "silk")]
pub mod silk;

#[cfg(feature = "celt")]
pub mod celt;

#[cfg(feature = "hybrid")]
pub mod modes;

pub mod plc;  // Always included
```

**Decoder implementation:**

```rust
impl Decoder {
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize, Error> {
        match self.mode {
            #[cfg(feature = "silk")]
            Mode::Silk => self.silk_decoder.decode(input, output, fec),

            #[cfg(feature = "celt")]
            Mode::Celt => self.celt_decoder.decode(input, output, fec),

            #[cfg(feature = "hybrid")]
            Mode::Hybrid => self.hybrid_decode(input, output, fec),

            #[cfg(not(any(feature = "silk", feature = "celt")))]
            _ => Err(Error::UnsupportedMode("mode disabled via feature flags".to_string())),
        }
    }
}
```

### Binary Size Impact

Approximate module sizes (estimated from RFC line counts):

| Feature          | Lines of Code | Impact                 |
| ---------------- | ------------- | ---------------------- |
| Range decoder    | ~500          | Always included (core) |
| SILK decoder     | ~4000         | Large (complex DSP)    |
| CELT decoder     | ~1000         | Medium (MDCT + PVQ)    |
| Mode integration | ~300          | Small                  |
| PLC              | ~100          | Small                  |

**Binary size optimization:**

- Speech-only: `--no-default-features --features silk` (~70% reduction)
- Music-only: `--no-default-features --features celt` (~60% reduction)

## API Compatibility Requirements

### Exact API Match with audiopus

For zero-cost re-exports to work, `moosicbox_opus_native` must match `audiopus` exactly:

```rust
// moosicbox_opus_native/src/lib.rs

pub enum Channels {
    Mono = 1,
    Stereo = 2,
}

pub enum SampleRate {
    Hz8000 = 8000,
    Hz12000 = 12000,
    Hz16000 = 16000,
    Hz24000 = 24000,
    Hz48000 = 48000,
}

#[derive(Debug)]
pub enum Error {
    // Must match audiopus::Error variants
}

pub struct Decoder {
    // Internal implementation
}

impl Decoder {
    // Must match audiopus::coder::Decoder::new signature EXACTLY
    pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self, Error>;

    // Must match audiopus::coder::Decoder::decode signature EXACTLY
    pub fn decode(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [i16],
        fec: bool,
    ) -> Result<usize, Error>;

    // Must match audiopus::coder::Decoder::decode_float signature EXACTLY
    pub fn decode_float(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [f32],
        fec: bool,
    ) -> Result<usize, Error>;

    // Must match audiopus GenericCtl::reset_state
    pub fn reset_state(&mut self) -> Result<(), Error>;
}
```

### Compile-Time API Verification

**Add to moosicbox_opus tests:**

```rust
// moosicbox_opus/tests/api_compatibility.rs

#[cfg(all(feature = "native", feature = "libopus"))]
compile_error!("Cannot test API compatibility with both backends enabled");

#[cfg(feature = "native")]
#[test]
fn native_decoder_api_matches() {
    use moosicbox_opus_native as native;

    // Type signatures must match exactly
    let _: fn(native::SampleRate, native::Channels) -> Result<native::Decoder, native::Error>
        = native::Decoder::new;
}
```

## Error Handling

### Error Types

```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid packet structure: {0}")]
    InvalidPacket(String),

    #[error("Unsupported mode: {0}")]
    UnsupportedMode(String),

    #[error("Decoder creation failed: {0}")]
    DecoderInit(String),

    #[error("Decode operation failed: {0}")]
    DecodeFailed(String),

    #[error("Range decoder error: {0}")]
    RangeDecoder(String),

    #[error("SILK decoder error: {0}")]
    SilkDecoder(String),

    #[error("CELT decoder error: {0}")]
    CeltDecoder(String),
}
```

### Error Propagation

- Use `Result<T, Error>` for all fallible operations
- Use `thiserror` for error type definitions
- Provide context in error messages
- Never panic in library code (except stub backend)

## Testing Strategy

### Unit Tests

- Every RFC section gets dedicated tests
- Test each function with valid/invalid inputs
- Test boundary conditions
- Test error paths

### Integration Tests

- End-to-end decode tests with real packets
- Mode switching tests
- State reset tests
- Packet loss scenarios

### Conformance Tests

- RFC 6716 test vectors (when available)
- Opus test suite integration
- Bit-exact output comparison with libopus

### Fuzzing

- AFL/libFuzzer on packet inputs
- Targeted fuzzing for each decoder module
- Continuous fuzzing in CI

### Performance Tests

- Benchmark critical paths (MDCT, LPC synthesis, etc.)
- Compare against libopus baseline
- Track performance regressions

### API Compatibility Tests

- Compile-time signature verification
- Runtime behavior comparison
- Error type compatibility

## Design Principles

1. **Correctness First**: Get RFC compliance perfect before optimizing
2. **Zero Unsafe**: No unsafe code unless absolutely necessary (SIMD)
3. **Zero Cost**: Backend selection must have zero runtime overhead
4. **Modular Design**: Each component testable in isolation
5. **Feature Gating**: Support partial compilation for binary size
6. **API Compatibility**: Exact match with audiopus for drop-in replacement
7. **Clear Errors**: Provide actionable error messages with context
8. **Documentation**: Document all DSP algorithms with RFC references
9. **No Compromise**: Halt on any RFC deviation

## Future Optimizations

**Post-correctness optimizations:**

- SIMD acceleration (safe wrappers for vectorization)
- Memory allocation reduction
- Algorithmic improvements where allowed by RFC
- Lookup table optimization
- Parallel frame decoding (if applicable)

**Optimization constraints:**

- Must maintain RFC bit-exact compliance
- Must maintain API compatibility
- Must maintain zero-cost abstraction
- Must not introduce unsafe code (except SIMD wrappers)
- Must include tests validating optimization correctness
