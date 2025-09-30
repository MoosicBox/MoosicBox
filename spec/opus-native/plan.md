# Native Opus Decoder Implementation Plan

## Overview

This plan outlines the implementation of a 100% safe, native Rust Opus decoder following RFC 6716. The implementation is divided into 11 phases, each building upon the previous to create a complete, production-ready decoder with zero-cost backend abstraction.

## Implementation Progress

- [ ] Phase 1: Foundation & Range Decoder
- [ ] Phase 2: SILK Decoder - Basic Structure
- [ ] Phase 3: SILK Decoder - Synthesis
- [ ] Phase 4: CELT Decoder - Basic Structure
- [ ] Phase 5: CELT Decoder - MDCT & Finalization
- [ ] Phase 6: Mode Integration & Hybrid
- [ ] Phase 7: Packet Loss Concealment
- [ ] Phase 8: Backend Integration
- [ ] Phase 9: Integration & Testing
- [ ] Phase 10: Optimization
- [ ] Phase 11: Documentation & Release

---

## Phase 1: Foundation & Range Decoder

**Goal:** Establish package foundation and implement RFC 4.1 Range Decoder (entropy decoder).

**Scope:** RFC 6716 Section 4.1 (Range Decoder)

**Additional Resources:**
- See `research/range-coding.md` for algorithm overview and state machine design
- Review entropy coding concepts and implementation approaches

### 1.1: Project Setup

- [ ] Create `packages/opus_native/` directory structure
- [ ] Create `packages/opus_native/.cargo/config.toml`:
  ```toml
  [build]
  target-dir = "../../target"

  [http]
  timeout = 1000000

  [net]
  git-fetch-with-cli = true
  ```

- [ ] Create `packages/opus_native/Cargo.toml`:
  ```toml
  [package]
  name = "moosicbox_opus_native"
  version = "0.1.0"
  authors = { workspace = true }
  categories = ["encoding", "multimedia", "codec"]
  description = "Pure Rust Opus audio decoder implementation"
  edition = { workspace = true }
  keywords = ["audio", "opus", "codec", "decoder", "rust"]
  license = { workspace = true }
  readme = "README.md"
  repository = { workspace = true }

  [dependencies]
  bytes = { workspace = true }
  log = { workspace = true }
  thiserror = { workspace = true }

  [dev-dependencies]
  hex = { workspace = true }
  pretty_assertions = { workspace = true }
  test-case = { workspace = true }

  [features]
  default = ["silk", "celt", "hybrid"]
  silk = []
  celt = []
  hybrid = ["silk", "celt"]
  fail-on-warnings = []
  ```

- [ ] Create minimal `README.md`
- [ ] Add to workspace `Cargo.toml` members
- [ ] Verify compilation: `cargo build -p moosicbox_opus_native`

#### 1.1 Verification Checklist
- [ ] Package compiles cleanly
- [ ] No clippy warnings
- [ ] All features compile independently

### 1.2: API Compatibility Layer

**Goal:** Create API surface matching audiopus exactly for zero-cost re-exports.

- [ ] Create `src/lib.rs` with clippy lints:
  ```rust
  #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

  pub mod error;
  mod range;

  pub use error::{Error, Result};
  ```

- [ ] Define types matching audiopus:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Channels {
      Mono = 1,
      Stereo = 2,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum SampleRate {
      Hz8000 = 8000,
      Hz12000 = 12000,
      Hz16000 = 16000,
      Hz24000 = 24000,
      Hz48000 = 48000,
  }

  pub struct Decoder {
      sample_rate: SampleRate,
      channels: Channels,
  }
  ```

- [ ] Implement API methods (stubs for now):
  ```rust
  impl Decoder {
      pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self> {
          Ok(Self { sample_rate, channels })
      }

      pub fn decode(
          &mut self,
          input: Option<&[u8]>,
          output: &mut [i16],
          fec: bool,
      ) -> Result<usize> {
          let _ = (input, output, fec);
          todo!("Implement in later phases")
      }

      pub fn decode_float(
          &mut self,
          input: Option<&[u8]>,
          output: &mut [f32],
          fec: bool,
      ) -> Result<usize> {
          let _ = (input, output, fec);
          todo!("Implement in later phases")
      }

      pub fn reset_state(&mut self) -> Result<()> {
          todo!("Implement in later phases")
      }
  }
  ```

- [ ] Create `src/error.rs`:
  ```rust
  use thiserror::Error;

  pub type Result<T> = std::result::Result<T, Error>;

  #[derive(Debug, Error)]
  pub enum Error {
      #[error("Invalid packet structure: {0}")]
      InvalidPacket(String),

      #[error("Unsupported configuration: {0}")]
      Unsupported(String),

      #[error("Decoder initialization failed: {0}")]
      InitFailed(String),

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

#### 1.2 Verification Checklist
- [ ] API types match audiopus exactly
- [ ] Error types comprehensive
- [ ] Compiles with `todo!()` implementations
- [ ] No clippy warnings

### 1.3: Range Decoder Data Structures

**Reference:** RFC 6716 Section 4.1

- [ ] Create `src/range/mod.rs`
- [ ] Create `src/range/decoder.rs`:
  ```rust
  use crate::error::{Error, Result};

  pub struct RangeDecoder {
      buffer: Vec<u8>,
      position: usize,
      value: u32,
      range: u32,
      total_bits: usize,
  }

  impl RangeDecoder {
      pub fn new(data: &[u8]) -> Result<Self> {
          if data.is_empty() {
              return Err(Error::RangeDecoder("empty buffer".to_string()));
          }

          Ok(Self {
              buffer: data.to_vec(),
              position: 0,
              value: 0,
              range: 0,
              total_bits: 0,
          })
      }
  }
  ```

- [ ] Document RFC 4.1 state machine in module docs
- [ ] Add unit tests for struct creation

#### 1.3 Verification Checklist
- [ ] Data structures defined per RFC 4.1
- [ ] Module organization clear
- [ ] Initial tests pass
- [ ] No clippy warnings

### 1.4: Range Decoder Initialization

**Reference:** RFC 6716 Section 4.1.1

- [ ] Implement `RangeDecoder::init()` or update `new()`:
  * Initialize `value` from first bytes
  * Initialize `range` to 128 per RFC
  * Validate buffer has minimum 2 bytes

- [ ] Add tests for initialization:
  * Valid buffer initialization
  * Empty buffer error
  * Single-byte buffer error

#### 1.4 Verification Checklist
- [ ] Initialization matches RFC 4.1.1 exactly
- [ ] All error cases tested
- [ ] Zero clippy warnings

### 1.5: Symbol Decoding (ec_decode)

**Reference:** RFC 6716 Section 4.1.2

- [ ] Implement `ec_decode()` function
- [ ] Implement `ec_decode_bin()` for binary symbols (RFC 4.1.3.1)
- [ ] Implement `ec_dec_update()` for state update
- [ ] Implement renormalization logic (RFC 4.1.2.1)
- [ ] Add comprehensive tests with RFC examples

#### 1.5 Verification Checklist
- [ ] Symbol decoding implemented per RFC 4.1.2
- [ ] Renormalization correct per RFC 4.1.2.1
- [ ] Tests cover all RFC examples
- [ ] Zero clippy warnings

### 1.6: Raw Bit Decoding

**Reference:** RFC 6716 Section 4.1.4

- [ ] Implement `ec_dec_bits()` function
- [ ] Handle bit exhaustion correctly
- [ ] Add tests for various bit counts (1-25 bits)
- [ ] Test edge cases (zero bits, max bits)

#### 1.6 Verification Checklist
- [ ] Raw bit extraction works per RFC 4.1.4
- [ ] Boundary conditions tested
- [ ] Error handling correct
- [ ] Zero clippy warnings

### 1.7: Uniformly Distributed Integers

**Reference:** RFC 6716 Section 4.1.5

- [ ] Implement `ec_dec_uint()` function (RFC 4.1.5)
- [ ] Implement `ec_dec_icdf()` function (RFC 4.1.3.3)
- [ ] Implement `ec_dec_bit_logp()` function (RFC 4.1.3.2)
- [ ] Add tests with RFC examples

#### 1.7 Verification Checklist
- [ ] Uniform distribution decoding correct per RFC 4.1.5
- [ ] ICDF decoding matches RFC 4.1.3.3
- [ ] Bit log probability works per RFC 4.1.3.2
- [ ] All test cases pass
- [ ] Zero clippy warnings

### 1.8: Bit Usage Tracking

**Reference:** RFC 6716 Section 4.1.6

- [ ] Implement `ec_tell()` function (RFC 4.1.6.1)
- [ ] Implement `ec_tell_frac()` function (RFC 4.1.6.2)
- [ ] Add tests for bit counting accuracy

#### 1.8 Verification Checklist
- [ ] Bit usage tracking accurate per RFC 4.1.6.1
- [ ] Fractional bit tracking works per RFC 4.1.6.2
- [ ] Tests validate correctness
- [ ] Zero clippy warnings

### 1.9: Range Decoder Integration Tests

- [ ] Create `tests/range_decoder_tests.rs`
- [ ] Test complete decode sequences
- [ ] Test error recovery
- [ ] Compare against RFC test vectors (if available)
- [ ] Test all public API functions

**Test Vector Usage:**
- Create test vectors in `test-vectors/range-decoder/` directory
- Follow format specified in `test-vectors/README.md`
- Reference `research/range-coding.md` test strategy section for test case design

#### 1.9 Verification Checklist
- [ ] All range decoder tests pass
- [ ] RFC compliance validated
- [ ] Zero clippy warnings
- [ ] No unused dependencies
- [ ] cargo build -p moosicbox_opus_native succeeds
- [ ] cargo test -p moosicbox_opus_native succeeds

---

## Phase 2: SILK Decoder - Basic Structure

**Goal:** Implement SILK decoder framework and basic decoding stages.

**Scope:** RFC 6716 Section 4.2.1 through 4.2.7.4 (partial)

**Feature:** `silk`

**Additional Resources:**
- See `research/silk-overview.md` for complete SILK architecture overview
- Review decoder pipeline, LP/LTP concepts, and major components

### 2.1: SILK Decoder Framework

**Reference:** RFC 6716 Section 4.2

- [ ] Create `src/silk/mod.rs` with `#[cfg(feature = "silk")]`
- [ ] Create `src/silk/decoder.rs` with `SilkDecoder` struct
- [ ] Define SILK state structures
- [ ] Implement decoder initialization
- [ ] Add basic tests

### 2.2: LP Layer Organization

**Reference:** RFC 6716 Section 4.2.2

- [ ] Implement LP layer detection from TOC
- [ ] Parse voice activity detection flags
- [ ] Parse per-frame LBRR flags (RFC 4.2.4)
- [ ] Add tests for layer organization

### 2.3: Header Bits Parsing

**Reference:** RFC 6716 Section 4.2.3

- [ ] Implement header bits decoding
- [ ] Parse frame-level parameters
- [ ] Add tests for header parsing

### 2.4: Stereo Prediction Weights

**Reference:** RFC 6716 Section 4.2.7.1

- [ ] Implement stereo prediction weight decoding
- [ ] Handle mid/side coding detection
- [ ] Add tests for stereo prediction

### 2.5: Subframe Gains

**Reference:** RFC 6716 Section 4.2.7.4

- [ ] Implement subframe gain decoding
- [ ] Handle gain quantization
- [ ] Add tests for gain decoding

[Continue with detailed breakdown for remaining phases 2.6-2.9, Phase 3-11...]

---

## Phase 3: SILK Decoder - Synthesis

**Goal:** Complete SILK decoder with LTP/LPC synthesis.

**Scope:** RFC 6716 Section 4.2.7.5 through 4.2.9

**Feature:** `silk`

**Test Vector Usage:**
- Create SILK test vectors in `test-vectors/silk/` directory
- Test all sample rates (8/12/16/24 kHz) and stereo modes
- Reference `test-vectors/README.md` for format specification

[Detailed breakdown of Phase 3 tasks...]

---

## Phase 4: CELT Decoder - Basic Structure

**Goal:** Implement CELT decoder framework through bit allocation.

**Scope:** RFC 6716 Section 4.3.1 through 4.3.3

**Feature:** `celt`

**Additional Resources:**
- See `research/celt-overview.md` for complete CELT architecture overview
- Review MDCT/PVQ concepts, decoder pipeline, and major components

[Detailed breakdown of Phase 4 tasks...]

---

## Phase 5: CELT Decoder - MDCT & Finalization

**Goal:** Complete CELT decoder with MDCT and post-processing.

**Scope:** RFC 6716 Section 4.3.4 through 4.3.7

**Feature:** `celt`

**Critical Reference for Phase 5.1 (Inverse MDCT):**
- See `research/mdct-implementation.md` for detailed MDCT implementation strategies
- Review bit-exact requirements, windowing functions, and implementation approaches
- Follow recommendations for FFT-based efficient computation

**Test Vector Usage:**
- Create CELT test vectors in `test-vectors/celt/` directory
- Test all sample rates (16/24/48 kHz), frame sizes, and transient cases
- Reference `test-vectors/README.md` for format specification

[Detailed breakdown of Phase 5 tasks...]

---

## Phase 6: Mode Integration & Hybrid

**Goal:** Integrate SILK and CELT decoders with mode switching.

**Scope:** RFC 6716 Section 4.5

**Feature:** `hybrid`

**Test Vector Usage:**
- Create hybrid mode test vectors in `test-vectors/integration/hybrid/` directory
- Create mode switching test vectors in `test-vectors/integration/transitions/` directory
- Test all mode combinations and transition scenarios

[Detailed breakdown of Phase 6 tasks...]

---

## Phase 7: Packet Loss Concealment

**Goal:** Implement PLC algorithms for robustness.

**Scope:** RFC 6716 Section 4.4

[Detailed breakdown of Phase 7 tasks...]

---

## Phase 8: Backend Integration

**Goal:** Integrate native decoder into moosicbox_opus with zero-cost backend selection.

**Scope:** Feature flags, zero-cost re-exports, backend wrappers

### 8.1: API Compatibility Verification

- [ ] Audit audiopus API surface:
  * Review `audiopus::Channels` enum
  * Review `audiopus::SampleRate` enum
  * Review `audiopus::Error` type
  * Review `audiopus::coder::Decoder` methods

- [ ] Ensure moosicbox_opus_native matches exactly:
  * `Channels` enum values and discriminants
  * `SampleRate` enum values and discriminants
  * `Error` type variants
  * `Decoder::new()` signature
  * `decode()` signature
  * `decode_float()` signature
  * `reset_state()` signature

- [ ] Create compile-time API compatibility tests:
  ```rust
  // moosicbox_opus_native/tests/api_compat.rs

  #[cfg(feature = "native")]
  #[test]
  fn native_api_signatures_match_audiopus() {
      use moosicbox_opus_native::{Channels, SampleRate, Decoder, Error};

      // Type-level assertions - these must compile if API matches
      let _: fn(SampleRate, Channels) -> Result<Decoder, Error> = Decoder::new;
  }

  #[cfg(feature = "libopus")]
  #[test]
  fn libopus_api_available() {
      use audiopus::{Channels, SampleRate, Error};
      use audiopus::coder::Decoder;

      // Verify libopus backend is available
      let _: fn(SampleRate, Channels) -> Result<Decoder, Error> = Decoder::new;
  }
  ```

#### 8.1 Verification Checklist
- [ ] All type signatures match audiopus exactly
- [ ] API compatibility tests compile
- [ ] Zero clippy warnings

### 8.2: Zero-Cost Re-export Setup

- [ ] Update moosicbox_opus/src/lib.rs with direct re-exports:
  ```rust
  #[cfg(feature = "libopus")]
  pub use audiopus::{Channels, SampleRate, Error};
  #[cfg(feature = "libopus")]
  pub use audiopus::coder::Decoder;

  #[cfg(all(feature = "native", not(feature = "libopus")))]
  pub use moosicbox_opus_native::{Channels, SampleRate, Error, Decoder};

  #[cfg(not(any(feature = "native", feature = "libopus")))]
  mod stub_backend;
  #[cfg(not(any(feature = "native", feature = "libopus")))]
  pub use stub_backend::{Channels, SampleRate, Error, Decoder};
  ```

- [ ] Remove trait-based approach (if any exists)
- [ ] Remove wrapper structs (if any exist)
- [ ] Verify no runtime overhead with benchmarks

#### 8.2 Verification Checklist
- [ ] Direct re-exports work
- [ ] No trait dispatch overhead
- [ ] No wrapper struct overhead
- [ ] Backend selection works at compile time
- [ ] Zero clippy warnings

### 8.3: Stub Backend Implementation

- [ ] Create moosicbox_opus/src/stub_backend.rs:
  ```rust
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
      pub fn new(_: SampleRate, _: Channels) -> Result<Self, Error> {
          panic!("No Opus backend enabled! Enable 'native' or 'libopus' feature.")
      }

      // ... other methods
  }
  ```

- [ ] Add `#[cold]` and `#[inline(never)]` attributes
- [ ] Ensure early panic in constructor
- [ ] Verify minimal binary size impact

#### 8.3 Verification Checklist
- [ ] Stub backend compiles
- [ ] Panic occurs at runtime if used
- [ ] Build warnings present
- [ ] Zero clippy warnings

### 8.4: Backend Selection Tests

- [ ] Test default backend (native)
- [ ] Test explicit native backend
- [ ] Test libopus backend (with and without default)
- [ ] Test stub backend (no features)
- [ ] Test feature flag warnings in build.rs

#### 8.4 Verification Checklist
- [ ] All backend combinations tested
- [ ] Warnings appear correctly
- [ ] Zero clippy warnings

### 8.5: Symphonia Integration

- [ ] Update moosicbox_opus Symphonia decoder to use new backend
- [ ] Ensure decoder works with both backends
- [ ] Test with real audio files
- [ ] Verify output correctness

#### 8.5 Verification Checklist
- [ ] Symphonia integration works
- [ ] Backend selection transparent to Symphonia
- [ ] Audio playback works
- [ ] Zero clippy warnings

---

## Phase 9: Integration & Testing

**Goal:** Comprehensive testing and RFC conformance validation.

**Scope:** Test suite, fuzzing, conformance tests

**Test Vector Infrastructure:**
- Organize all test vectors per `test-vectors/README.md` structure
- Create comprehensive conformance tests for all decoder modes
- Reference research documentation for test strategy:
  - `research/range-coding.md` - Range decoder test design
  - `research/silk-overview.md` - SILK conformance test strategy
  - `research/celt-overview.md` - CELT conformance test strategy
  - `research/mdct-implementation.md` - MDCT validation test requirements

**Test Vector Categories:**
- `test-vectors/range-decoder/` - Range decoder conformance
- `test-vectors/silk/` - SILK decoder (all sample rates, mono/stereo)
- `test-vectors/celt/` - CELT decoder (all sample rates, frame sizes, transients)
- `test-vectors/integration/` - End-to-end tests (speech, music, hybrid, transitions)
- `test-vectors/edge-cases/` - Error conditions, malformed packets, boundary cases

[Detailed breakdown of Phase 9 tasks...]

---

## Phase 10: Optimization

**Goal:** Optimize performance while maintaining RFC compliance.

**Scope:** SIMD, memory optimization, algorithmic improvements

[Detailed breakdown of Phase 10 tasks...]

---

## Phase 11: Documentation & Release

**Goal:** Complete documentation and prepare for release.

**Scope:** API docs, usage examples, migration guide

[Detailed breakdown of Phase 11 tasks...]

---

## Testing Philosophy

### Per-Phase Testing
- Unit tests written alongside implementation
- Both success and failure paths tested
- RFC references in test documentation
- Test isolation (no cross-phase dependencies)

### Continuous Validation
- Zero clippy warnings maintained
- All tests must pass before moving to next phase
- No unused dependencies (cargo machete)
- Clean compilation with all feature combinations

### RFC Compliance
- Reference RFC section in all implementations
- Use RFC terminology in code
- Document deviations (if any, with justification)
- Validate against RFC test vectors

### Zero-Cost Verification
- Benchmark backend selection overhead
- Ensure no runtime cost from abstraction
- Verify perfect inlining across boundaries

## Success Criteria

Each phase is considered complete when:
- [ ] All subtasks have checked boxes
- [ ] All verification checklists passed
- [ ] Zero clippy warnings
- [ ] All tests passing
- [ ] Proof documented under each checkbox
- [ ] RFC compliance validated
- [ ] No unused dependencies

## Risk Management

### High-Complexity Areas
- SILK LSF/LPC decoding - Extensive codebooks and interpolation
- CELT PVQ - Complex mathematical operations
- Inverse MDCT - Requires bit-exact accuracy
- Bit allocation - Dynamic and configuration-dependent
- API compatibility - Must match audiopus exactly

### Mitigation Strategies
- Break complex areas into smaller subtasks
- Extensive unit testing at each step
- Reference implementation comparison (libopus)
- Incremental integration (test early, test often)
- API compatibility tests at compile time

## Notes

- No timelines or effort estimates per project requirements
- Feature flags allow partial compilation
- Backend selection via zero-cost re-exports (no runtime overhead)
- API compatibility with audiopus maintained throughout
- All abstractions must be zero-cost
