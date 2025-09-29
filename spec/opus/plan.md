# Opus Codec Implementation Plan

## Executive Summary

This document provides a comprehensive implementation plan for Opus codec (RFC 6716) support in MoosicBox. The implementation leverages Symphonia's `Decoder` trait system with a custom `CodecRegistry` to avoid modifying the upstream Symphonia library.

**Status**: Planning Phase
**Complexity**: High - Requires RFC-compliant packet parsing, frame decoding, and audio pipeline integration
**Dependencies**: libopus (reference implementation), Symphonia 0.5+, audiopus/opus-rs bindings

## Phase 1: Package Structure and Dependencies

### 1.1 Create moosicbox_opus Package

**Location**: `/packages/opus/`

- [ ] Create directory structure:
  ```
  packages/opus/
  ├── Cargo.toml
  ├── src/
  │   ├── lib.rs           # Public API and module exports
  │   ├── decoder.rs       # OpusDecoder implementation
  │   ├── packet.rs        # Packet parsing (RFC 6716 Section 3)
  │   ├── toc.rs           # TOC byte parsing (Section 3.1)
  │   ├── frame.rs         # Frame structure and packing
  │   ├── range_decoder.rs # Range decoder wrapper
  │   ├── error.rs         # Error types
  │   └── registry.rs      # Custom codec registry
  └── tests/
      ├── packet_tests.rs
      ├── decoder_tests.rs
      └── fixtures/        # Test Opus files
  ```

### 1.2 Workspace Integration

- [ ] Update root `Cargo.toml` workspace members to include `"packages/opus"`
- [ ] Add workspace dependencies in root `Cargo.toml`:
  ```toml
  # Add to [workspace.dependencies] section
  moosicbox_opus = { version = "0.1.1", default-features = false, path = "packages/opus" }

  # External dependencies (add if not present)
  test-case = "3.1.0"  # For testing
  ```

### 1.3 Configure Package Cargo.toml

- [ ] Create `packages/opus/Cargo.toml`:
  ```toml
  [package]
  name = "moosicbox_opus"
  version = "0.1.1"
  authors = { workspace = true }
  categories = ["encoding", "multimedia", "codec"]
  description = "MoosicBox Opus codec decoder implementation for Symphonia"
  edition = { workspace = true }
  keywords = ["audio", "opus", "codec", "decoder", "symphonia"]
  license = { workspace = true }
  readme = "README.md"
  repository = { workspace = true }

  [dependencies]
  # Internal dependencies
  moosicbox_audio_decoder = { workspace = true, optional = true }

  # External dependencies from workspace
  audiopus = { workspace = true }
  bytes = { workspace = true }
  log = { workspace = true }
  symphonia = { workspace = true }
  thiserror = { workspace = true }

  [dev-dependencies]
  hex = { workspace = true }
  test-case = { workspace = true }
  insta = { workspace = true }
  pretty_assertions = { workspace = true }

  [features]
  default = []
  fail-on-warnings = []
  ```

### 1.4 Update audio_decoder Integration

- [ ] Add `moosicbox_opus = { workspace = true, optional = true }` to `packages/audio_decoder/Cargo.toml`
- [ ] Update features: `opus = ["dep:moosicbox_opus"]`

## Phase 2: RFC 6716 Packet Structure Implementation

### 2.1 TOC Byte Parser (`src/toc.rs`)

**RFC Reference**: Section 3.1

- [ ] Implement `TocByte` struct:
  ```rust
  pub struct TocByte {
      config: u8,      // bits 0-4: configuration number
      stereo: bool,    // bit 5: mono/stereo flag
      frame_code: u8,  // bits 6-7: frame count code
  }
  ```

- [ ] Implement configuration mapping (Table 2):
  ```rust
  pub enum OpusMode {
      SilkOnly { bandwidth: Bandwidth, frame_size_ms: f32 },
      Hybrid { bandwidth: Bandwidth, frame_size_ms: f32 },
      CeltOnly { bandwidth: Bandwidth, frame_size_ms: f32 },
  }

  pub enum Bandwidth {
      Narrowband,     // 4 kHz (config 0-3, 16-19)
      Mediumband,     // 6 kHz (config 4-7)
      Wideband,       // 8 kHz (config 8-11, 20-23)
      SuperWideband,  // 12 kHz (config 12-13, 24-27)
      Fullband,       // 20 kHz (config 14-15, 28-31)
  }
  ```

- [ ] Parse function with validation:
  ```rust
  impl TocByte {
      pub fn parse(byte: u8) -> Result<Self, OpusError> {
          // Extract fields
          // Validate configuration number (0-31)
          // Map to mode and parameters
      }
  }
  ```

### 2.2 Frame Packing Parser (`src/frame.rs`)

**RFC Reference**: Section 3.2

- [ ] Implement frame count codes:
  ```rust
  pub enum FramePacking {
      SingleFrame,                    // Code 0
      TwoFramesEqual,                 // Code 1
      TwoFramesVariable,              // Code 2
      ArbitraryFrames { count: u8 }, // Code 3
  }
  ```

- [ ] Frame length decoder (Section 3.2.1):
  ```rust
  pub fn decode_frame_length(data: &[u8]) -> Result<(usize, usize), OpusError> {
      match data[0] {
          0 => Ok((0, 1)),           // DTX
          1..=251 => Ok((data[0] as usize, 1)),
          252..=255 => {
              // Two-byte length encoding
              let length = (data[1] as usize * 4) + data[0] as usize;
              Ok((length, 2))
          }
      }
  }
  ```

- [ ] Implement packet validators [R1-R7]:
  ```rust
  pub fn validate_packet(data: &[u8]) -> Result<(), OpusError> {
      // [R1] At least one byte
      // [R2] Frame length ≤ 1275 bytes
      // [R3] Code 1: odd total length
      // [R4] Code 2: valid frame lengths
      // [R5] Code 3: 1+ frames, ≤120ms total
      // [R6] CBR Code 3: proper padding
      // [R7] VBR Code 3: sufficient data
  }
  ```

### 2.3 Packet Parser (`src/packet.rs`)

- [ ] Main packet structure:
  ```rust
  pub struct OpusPacket {
      toc: TocByte,
      frames: Vec<OpusFrame>,
      padding: Vec<u8>,
  }

  pub struct OpusFrame {
      data: Vec<u8>,
      is_dtx: bool,
  }
  ```

- [ ] Complete packet parser:
  ```rust
  impl OpusPacket {
      pub fn parse(data: &[u8]) -> Result<Self, OpusError> {
          validate_packet(data)?;
          let toc = TocByte::parse(data[0])?;
          let frames = match toc.frame_code {
              0 => parse_code_0(&data[1..])?,
              1 => parse_code_1(&data[1..])?,
              2 => parse_code_2(&data[1..])?,
              3 => parse_code_3(&data[1..])?,
              _ => unreachable!(),
          };
          Ok(OpusPacket { toc, frames, padding: vec![] })
      }
  }
  ```

## Phase 3: Symphonia Decoder Implementation

### 3.1 OpusDecoder Structure (`src/decoder.rs`)

- [ ] Implement core decoder struct:
  ```rust
  pub struct OpusDecoder {
      params: CodecParameters,
      opus_decoder: audiopus::coder::Decoder,
      output_buf: AudioBuffer<f32>,
      sample_rate: u32,
      channel_count: usize,
      frame_size_samples: usize,
  }
  ```

### 3.2 Decoder Trait Implementation

- [ ] Implement `symphonia::core::codecs::Decoder`:
  ```rust
  impl Decoder for OpusDecoder {
      fn try_new(params: &CodecParameters, options: &DecoderOptions)
          -> Result<Self>
      {
          // Extract sample rate (default 48000)
          let sample_rate = params.sample_rate.unwrap_or(48000);

          // Extract channel count
          let channels = match params.channels {
              Some(c) => c.count(),
              None => return Err(Error::Unsupported("missing channels")),
          };

          // Initialize libopus decoder
          let opus_decoder = audiopus::coder::Decoder::new(
              audiopus::SampleRate::try_from(sample_rate as i32)?,
              audiopus::Channels::try_from(channels)?,
          )?;

          // Calculate frame size from extra_data or default
          let frame_size_samples = calculate_frame_size(params)?;

          // Pre-allocate output buffer
          let spec = SignalSpec::new(sample_rate, channels.into());
          let output_buf = AudioBuffer::new(frame_size_samples as u64, spec);

          Ok(Self {
              params: params.clone(),
              opus_decoder,
              output_buf,
              sample_rate,
              channel_count: channels,
              frame_size_samples,
          })
      }

      fn reset(&mut self) {
          self.opus_decoder.reset_state().unwrap();
          self.output_buf.clear();
      }

      fn supported_codecs() -> &'static [CodecDescriptor] {
          &[support_codec!(CODEC_TYPE_OPUS, "opus", "Opus Interactive Audio Codec")]
      }

      fn codec_params(&self) -> &CodecParameters {
          &self.params
      }
  }
  ```

### 3.3 Decode Implementation

- [ ] Main decode function:
  ```rust
  fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
      // Clear previous buffer
      self.output_buf.clear();

      // Parse Opus packet structure
      let opus_packet = OpusPacket::parse(&packet.data)?;

      // Validate packet timing
      if let Some(dur) = packet.dur {
          self.validate_duration(dur, &opus_packet)?;
      }

      // Decode each frame
      let mut output_offset = 0;
      for frame in &opus_packet.frames {
          if frame.is_dtx {
              // Handle DTX (silence) frame
              self.handle_dtx_frame(output_offset)?;
          } else {
              // Decode audio frame
              let samples = self.decode_frame(&frame.data, output_offset)?;
              output_offset += samples;
          }
      }

      // Trim output buffer to actual decoded size
      self.output_buf.truncate(output_offset);

      Ok(self.output_buf.as_audio_buffer_ref())
  }
  ```

- [ ] Frame decoding helper:
  ```rust
  fn decode_frame(&mut self, data: &[u8], offset: usize)
      -> Result<usize, Error>
  {
      // Get mutable slice of output buffer
      let output = self.output_buf.chan_mut(0);
      let out_slice = &mut output[offset..];

      // Decode with libopus
      let decoded_samples = match self.channel_count {
          1 => self.opus_decoder.decode(data, out_slice, false)?,
          2 => {
              // Interleaved stereo decoding
              let mut interleaved = vec![0f32; self.frame_size_samples * 2];
              let samples = self.opus_decoder.decode(
                  data,
                  &mut interleaved,
                  false
              )?;

              // Deinterleave into planar format
              self.deinterleave_stereo(&interleaved, offset, samples)?;
              samples
          }
          _ => return Err(Error::Unsupported("channel count")),
      };

      Ok(decoded_samples)
  }
  ```

### 3.4 Packet Loss Concealment (PLC)

**RFC Reference**: Section 4.4

- [ ] Implement PLC for lost packets:
  ```rust
  fn handle_packet_loss(&mut self) -> Result<usize, Error> {
      // Use FEC if available in previous packet
      if let Some(fec_data) = self.get_fec_data() {
          self.opus_decoder.decode(fec_data, output, true)?
      } else {
          // Generate comfort noise
          self.opus_decoder.decode(&[], output, false)?
      }
  }
  ```

## Phase 4: Custom Codec Registry

### 4.1 Registry Implementation (`src/registry.rs`)

- [ ] Create custom registry:
  ```rust
  use symphonia::core::codecs::{CodecRegistry, CODEC_TYPE_OPUS};

  pub fn register_opus_codec(registry: &mut CodecRegistry) {
      registry.register(&support_codec!(
          CODEC_TYPE_OPUS,
          "opus",
          "Opus Interactive Audio Codec"
      ));
  }

  pub fn create_opus_registry() -> CodecRegistry {
      let mut registry = CodecRegistry::new();
      register_opus_codec(&mut registry);
      // Register other Symphonia codecs
      symphonia::default::register_enabled_codecs(&mut registry);
      registry
  }
  ```

### 4.2 Integration in audio_decoder

- [ ] Modify `packages/audio_decoder/src/lib.rs`:
  ```rust
  #[cfg(feature = "opus")]
  use moosicbox_opus::create_opus_registry;

  fn get_codec_registry() -> CodecRegistry {
      #[cfg(feature = "opus")]
      return create_opus_registry();

      #[cfg(not(feature = "opus"))]
      symphonia::default::get_codecs()
  }

  // Update line 495:
  let mut decoder = get_codec_registry().make(&track.codec_params, &decode_opts)?;
  ```

## Phase 5: Container Format Support

### 5.1 Ogg Opus Support

- [ ] Verify Symphonia's Ogg demuxer recognizes Opus:
  ```rust
  // Test with OggOpus files
  // Ensure codec parameters are properly extracted
  // Validate channel mapping
  ```

### 5.2 WebM/Matroska Support

- [ ] Test with WebM containers:
  ```rust
  // Ensure proper codec private data extraction
  // Handle Opus delay/padding from container
  ```

## Phase 6: Comprehensive Testing

### 6.1 Unit Tests (`tests/packet_tests.rs`)

- [ ] TOC byte parsing tests:
  ```rust
  #[test_case(0b00011001, 3, false, 1; "silk_nb_60ms_mono_single")]
  #[test_case(0b01111101, 15, true, 1; "hybrid_fb_20ms_stereo_equal")]
  #[test_case(0b11111110, 31, true, 2; "celt_fb_20ms_stereo_variable")]
  fn test_toc_parsing(byte: u8, config: u8, stereo: bool, code: u8) {
      let toc = TocByte::parse(byte).unwrap();
      assert_eq!(toc.config, config);
      assert_eq!(toc.stereo, stereo);
      assert_eq!(toc.frame_code, code);
  }
  ```

- [ ] Frame length encoding tests:
  ```rust
  #[test_case(&[100], 100, 1; "single_byte")]
  #[test_case(&[252, 1], 253, 2; "two_byte_min")]
  #[test_case(&[255, 255], 1275, 2; "two_byte_max")]
  fn test_frame_length(data: &[u8], expected_len: usize, bytes_read: usize) {
      let (len, read) = decode_frame_length(data).unwrap();
      assert_eq!(len, expected_len);
      assert_eq!(read, bytes_read);
  }
  ```

- [ ] Packet validation tests [R1-R7]:
  ```rust
  #[test]
  fn test_malformed_packets() {
      // Test each constraint violation
      assert!(validate_packet(&[]).is_err()); // [R1]
      assert!(validate_packet(&[0xFF; 1276]).is_err()); // [R2]
      // ... test all constraints
  }
  ```

### 6.2 Decoder Tests (`tests/decoder_tests.rs`)

- [ ] Basic decoding test:
  ```rust
  #[test]
  fn test_basic_decode() {
      let params = CodecParameters::new()
          .for_codec(CODEC_TYPE_OPUS)
          .with_sample_rate(48000)
          .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

      let decoder = OpusDecoder::try_new(&params, &Default::default()).unwrap();
      // Test with known Opus packet
  }
  ```

- [ ] Test all Opus modes:
  ```rust
  #[test_case("silk_nb.opus"; "SILK narrowband")]
  #[test_case("hybrid_swb.opus"; "Hybrid super-wideband")]
  #[test_case("celt_fb.opus"; "CELT fullband")]
  fn test_opus_modes(fixture: &str) {
      // Decode and verify output
  }
  ```

### 6.3 RFC Test Vectors

- [ ] Implement RFC 6716 Appendix A test vectors:
  ```rust
  const TEST_VECTOR_1: &[u8] = &[0xFC, 0x00, 0x00, ...];

  #[test]
  fn test_rfc_vectors() {
      // Decode test vectors
      // Compare with expected PCM output
      // Verify bit-exact compliance
  }
  ```

### 6.4 Integration Tests

- [ ] End-to-end playback test:
  ```rust
  #[tokio::test]
  async fn test_opus_playback() {
      // Open Opus file
      // Decode through MoosicBox pipeline
      // Verify audio output
  }
  ```

- [ ] Memory leak test:
  ```rust
  #[test]
  fn test_decoder_memory_leak() {
      // Decode large file
      // Monitor memory usage
      // Verify proper cleanup
  }
  ```

## Phase 7: Performance and Optimization

### 7.1 Benchmarking

- [ ] Create benchmarks with Criterion:
  ```rust
  // Add to workspace Cargo.toml: criterion = "0.5.1"
  // Add to package Cargo.toml:
  [dev-dependencies]
  criterion = { workspace = true }

  [[bench]]
  name = "opus_benchmarks"
  harness = false
  ```

- [ ] Benchmark implementation:
  ```rust
  #[bench]
  fn bench_packet_parsing(b: &mut Bencher) {
      let packet = create_test_packet();
      b.iter(|| OpusPacket::parse(&packet));
  }

  #[bench]
  fn bench_frame_decode(b: &mut Bencher) {
      let mut decoder = create_test_decoder();
      let packet = create_test_packet();
      b.iter(|| decoder.decode(&packet));
  }
  ```

### 7.2 Optimization Targets

- [ ] Profile hot paths with `perf`/`flamegraph`
- [ ] Optimize memory allocations:
  - Pre-allocate buffers
  - Reuse frame data structures
  - Minimize vector reallocations
- [ ] SIMD optimizations for deinterleaving
- [ ] Zero-copy packet parsing where possible

## Phase 8: Error Handling and Resilience

### 8.1 Error Types (`src/error.rs`)

- [ ] Comprehensive error enum:
  ```rust
  #[derive(Debug, Error)]
  pub enum OpusError {
      #[error("Invalid TOC byte: {0:#x}")]
      InvalidToc(u8),

      #[error("Invalid frame length: {0} bytes (max 1275)")]
      InvalidFrameLength(usize),

      #[error("Packet too short: {0} bytes")]
      PacketTooShort(usize),

      #[error("Decoder error: {0}")]
      DecoderError(#[from] audiopus::Error),

      #[error("Symphonia error: {0}")]
      SymphoniaError(#[from] symphonia::core::errors::Error),

      #[error("Unsupported configuration: {0}")]
      UnsupportedConfig(String),

      #[error("IO error: {0}")]
      IoError(#[from] std::io::Error),
  }
  ```

### 8.2 Graceful Degradation

- [ ] Handle corrupted packets:
  ```rust
  fn handle_decode_error(&mut self, error: OpusError) -> AudioBufferRef {
      log::warn!("Opus decode error: {}", error);

      // Try packet loss concealment
      if let Ok(samples) = self.handle_packet_loss() {
          return self.output_buf.as_audio_buffer_ref();
      }

      // Return silence
      self.output_buf.clear();
      self.output_buf.as_audio_buffer_ref()
  }
  ```

## Phase 9: Documentation and Examples

### 9.1 API Documentation

- [ ] Comprehensive rustdoc:
  ```rust
  /// Opus audio codec decoder implementing RFC 6716.
  ///
  /// # Features
  /// * SILK mode for speech (NB/MB/WB)
  /// * CELT mode for music (NB/WB/SWB/FB)
  /// * Hybrid mode for mixed content
  /// * Packet loss concealment
  /// * Forward error correction
  ///
  /// # Example
  /// ```rust
  /// let decoder = OpusDecoder::try_new(&params, &options)?;
  /// let audio = decoder.decode(&packet)?;
  /// ```
  ```

### 9.2 Module Organization (`src/lib.rs`)

- [ ] Create lib.rs:
  ```rust
  #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
  #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
  #![allow(clippy::multiple_crate_versions)]

  //! # MoosicBox Opus Codec
  //!
  //! RFC 6716 compliant Opus audio codec decoder for Symphonia.

  pub mod decoder;
  pub mod error;
  pub mod frame;
  pub mod packet;
  pub mod registry;
  pub mod toc;

  pub use decoder::OpusDecoder;
  pub use error::OpusError;
  pub use registry::{create_opus_registry, register_opus_codec};
  ```

### 9.3 Usage Examples

- [ ] Create example applications:
  - `examples/decode_opus.rs` - Basic decoding
  - `examples/opus_to_wav.rs` - Transcode to WAV
  - `examples/opus_stream.rs` - Network streaming

## Validation Criteria

### RFC Compliance Checklist

- [ ] TOC byte parsing matches Table 2 configurations
- [ ] Frame packing codes 0-3 correctly implemented
- [ ] Frame length encoding handles all cases (0-1275)
- [ ] Packet validation enforces constraints [R1-R7]
- [ ] Bandwidth modes correctly mapped (NB/MB/WB/SWB/FB)
- [ ] Frame durations accurate (2.5/5/10/20/40/60 ms)
- [ ] Stereo handling matches specification
- [ ] DTX (silence) frames properly handled

### Test Coverage Requirements

- [ ] Unit test coverage ≥95% for packet parsing
- [ ] Unit test coverage ≥90% for decoder logic
- [ ] All RFC test vectors passing
- [ ] Integration tests with real Opus files
- [ ] Fuzz testing for malformed packets
- [ ] Memory leak tests passing
- [ ] Performance benchmarks established

### Integration Requirements

- [ ] Seamless integration with existing codecs
- [ ] No regression in other codec support
- [ ] Proper error propagation to application layer
- [ ] Logging at appropriate levels
- [ ] Configuration via feature flags
- [ ] Documentation complete and accurate

## Risk Mitigation

### Technical Risks

1. **libopus compatibility**: Test with multiple versions
2. **Memory safety**: Use safe Rust patterns, avoid unsafe
3. **Performance**: Profile early and often
4. **Container format issues**: Test with various muxers

### Implementation Risks

1. **Scope creep**: Stick to RFC requirements
2. **Testing gaps**: Implement tests alongside code
3. **Integration issues**: Test incrementally
4. **Documentation debt**: Document as you code
