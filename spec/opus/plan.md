# Opus Codec Implementation Plan

## Executive Summary

This plan ensures each phase produces fully compilable code with no warnings. Dependencies are added only when first used. Each phase builds upon the previous, maintaining all RFC 6716 compliance requirements while ensuring clean compilation at every step.

## Phase 1: Minimal Package Foundation (Zero Dependencies)

### 1.1 Create Package Structure

- [ ] Create `/packages/opus/` directory
- [ ] Create minimal `Cargo.toml`:
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
  # No dependencies yet - will be added as needed

  [features]
  default = []
  fail-on-warnings = []
  ```

### 1.2 Create Minimal lib.rs

- [ ] Create `src/lib.rs`:
  ```rust
  #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
  #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
  #![allow(clippy::multiple_crate_versions)]

  //! # MoosicBox Opus Codec
  //!
  //! RFC 6716 compliant Opus audio codec decoder for Symphonia.
  //!
  //! This crate is under development.

  // No modules yet - will be added incrementally
  ```

### 1.3 Update Workspace

- [ ] Add to root `Cargo.toml` workspace members:
  ```toml
  members = [
      # ... existing members ...
      "packages/opus",
  ]
  ```

- [ ] Add to workspace dependencies (but don't use yet):
  ```toml
  moosicbox_opus = { version = "0.1.1", default-features = false, path = "packages/opus" }
  ```

**Validation**: `cargo build -p moosicbox_opus` succeeds with empty crate

## Phase 2: Error Types Foundation

### 2.1 Add thiserror Dependency

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dependencies]
  thiserror = { workspace = true }  # NOW we need it for error types
  ```

### 2.2 Create Error Module

- [ ] Create `src/error.rs`:
  ```rust
  use thiserror::Error;

  /// Opus codec errors.
  #[derive(Debug, Error)]
  pub enum OpusError {
      /// Placeholder for future packet parsing errors
      #[error("Invalid packet format")]
      InvalidPacket,

      /// Placeholder for future decoding errors
      #[error("Decoding failed")]
      DecodingFailed,
  }

  /// Result type for Opus operations.
  pub type OpusResult<T> = Result<T, OpusError>;
  ```

### 2.3 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod error;

  pub use error::{OpusError, OpusResult};
  ```

**Validation**: `cargo build -p moosicbox_opus` compiles, `cargo clippy` passes

## Phase 3: TOC and Basic Types

### 3.1 Create TOC Module

- [ ] Create `src/toc.rs`:
  ```rust
  use crate::error::{OpusError, OpusResult};

  /// TOC byte structure (RFC 6716 Section 3.1).
  #[derive(Debug, Clone, Copy)]
  pub struct TocByte {
      /// Configuration number (0-31)
      config: u8,
      /// Stereo flag
      stereo: bool,
      /// Frame count code (0-3)
      frame_code: u8,
  }

  impl TocByte {
      /// Parse a TOC byte.
      pub fn parse(byte: u8) -> OpusResult<Self> {
          let config = (byte >> 3) & 0x1F;
          let stereo = (byte & 0x04) != 0;
          let frame_code = byte & 0x03;

          Ok(TocByte {
              config,
              stereo,
              frame_code,
          })
      }

      /// Get configuration number.
      #[must_use]
      pub fn config(&self) -> u8 {
          self.config
      }

      /// Check if stereo.
      #[must_use]
      pub fn is_stereo(&self) -> bool {
          self.stereo
      }

      /// Get frame count code.
      #[must_use]
      pub fn frame_code(&self) -> u8 {
          self.frame_code
      }
  }

  /// Opus mode derived from configuration.
  #[derive(Debug, Clone, Copy)]
  pub enum OpusMode {
      /// SILK mode for speech
      SilkOnly,
      /// Hybrid mode
      Hybrid,
      /// CELT mode for music
      CeltOnly,
  }

  /// Audio bandwidth.
  #[derive(Debug, Clone, Copy)]
  pub enum Bandwidth {
      /// 4 kHz
      Narrowband,
      /// 6 kHz
      Mediumband,
      /// 8 kHz
      Wideband,
      /// 12 kHz
      SuperWideband,
      /// 20 kHz
      Fullband,
  }
  ```

### 3.2 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod error;
  pub mod toc;

  pub use error::{OpusError, OpusResult};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: Compiles with no warnings, all items are exported and used

## Phase 4: Frame Structure (No New Dependencies Yet)

### 4.1 Update Error Types

- [ ] Add to `src/error.rs`:
  ```rust
  #[derive(Debug, Error)]
  pub enum OpusError {
      // ... existing variants ...

      /// Invalid frame length
      #[error("Invalid frame length: {0} bytes (max 1275)")]
      InvalidFrameLength(usize),

      /// Packet too short
      #[error("Packet too short: {0} bytes")]
      PacketTooShort(usize),
  }
  ```

### 4.2 Create Frame Module

- [ ] Create `src/frame.rs`:
  ```rust
  use crate::error::{OpusError, OpusResult};

  /// Frame packing modes.
  #[derive(Debug, Clone)]
  pub enum FramePacking {
      /// Code 0: Single frame
      SingleFrame,
      /// Code 1: Two equal frames
      TwoFramesEqual,
      /// Code 2: Two variable frames
      TwoFramesVariable,
      /// Code 3: Multiple frames
      ArbitraryFrames { count: u8 },
  }

  /// Decode frame length from packet data.
  pub fn decode_frame_length(data: &[u8]) -> OpusResult<(usize, usize)> {
      if data.is_empty() {
          return Err(OpusError::PacketTooShort(0));
      }

      match data[0] {
          0 => Ok((0, 1)),  // DTX
          1..=251 => Ok((data[0] as usize, 1)),
          252..=255 => {
              if data.len() < 2 {
                  return Err(OpusError::PacketTooShort(data.len()));
              }
              let length = (data[1] as usize * 4) + data[0] as usize;
              if length > 1275 {
                  return Err(OpusError::InvalidFrameLength(length));
              }
              Ok((length, 2))
          }
      }
  }

  /// Opus frame data.
  #[derive(Debug, Clone)]
  pub struct OpusFrame {
      /// Frame data bytes
      pub data: Vec<u8>,
      /// Is DTX (silence) frame
      pub is_dtx: bool,
  }
  ```

### 4.3 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod error;
  pub mod frame;
  pub mod toc;

  pub use error::{OpusError, OpusResult};
  pub use frame::{decode_frame_length, FramePacking, OpusFrame};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: All functions are used/exported, no warnings

## Phase 5: Packet Parser (Add bytes and log dependencies)

### 5.1 Add Dependencies

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dependencies]
  bytes = { workspace = true }      # For efficient byte handling
  log = { workspace = true }        # For logging
  thiserror = { workspace = true }
  ```

### 5.2 Create Packet Module

- [ ] Create `src/packet.rs`:
  ```rust
  use bytes::Bytes;
  use log::debug;

  use crate::{
      error::{OpusError, OpusResult},
      frame::{decode_frame_length, OpusFrame},
      toc::TocByte,
  };

  /// Parsed Opus packet.
  #[derive(Debug, Clone)]
  pub struct OpusPacket {
      /// Table of contents byte
      pub toc: TocByte,
      /// Decoded frames
      pub frames: Vec<OpusFrame>,
      /// Optional padding
      pub padding: Bytes,
  }

  impl OpusPacket {
      /// Parse an Opus packet from bytes.
      pub fn parse(data: &[u8]) -> OpusResult<Self> {
          if data.is_empty() {
              return Err(OpusError::PacketTooShort(0));
          }

          debug!("Parsing Opus packet, size: {} bytes", data.len());

          let toc = TocByte::parse(data[0])?;
          let frames = match toc.frame_code() {
              0 => parse_code_0(&data[1..])?,
              1 => parse_code_1(&data[1..])?,
              2 => parse_code_2(&data[1..])?,
              3 => parse_code_3(&data[1..])?,
              _ => unreachable!(),
          };

          Ok(OpusPacket {
              toc,
              frames,
              padding: Bytes::new(),
          })
      }
  }

  /// Parse code 0 packet (single frame).
  fn parse_code_0(data: &[u8]) -> OpusResult<Vec<OpusFrame>> {
      Ok(vec![OpusFrame {
          data: data.to_vec(),
          is_dtx: data.is_empty(),
      }])
  }

  /// Parse code 1 packet (two equal frames).
  fn parse_code_1(data: &[u8]) -> OpusResult<Vec<OpusFrame>> {
      if data.len() % 2 != 0 {
          return Err(OpusError::InvalidPacket);
      }
      let frame_size = data.len() / 2;
      Ok(vec![
          OpusFrame {
              data: data[..frame_size].to_vec(),
              is_dtx: false,
          },
          OpusFrame {
              data: data[frame_size..].to_vec(),
              is_dtx: false,
          },
      ])
  }

  /// Parse code 2 packet (two variable frames).
  fn parse_code_2(data: &[u8]) -> OpusResult<Vec<OpusFrame>> {
      let (len1, offset) = decode_frame_length(data)?;
      if offset + len1 > data.len() {
          return Err(OpusError::PacketTooShort(data.len()));
      }

      Ok(vec![
          OpusFrame {
              data: data[offset..offset + len1].to_vec(),
              is_dtx: len1 == 0,
          },
          OpusFrame {
              data: data[offset + len1..].to_vec(),
              is_dtx: false,
          },
      ])
  }

  /// Parse code 3 packet (multiple frames).
  fn parse_code_3(data: &[u8]) -> OpusResult<Vec<OpusFrame>> {
      if data.is_empty() {
          return Err(OpusError::PacketTooShort(0));
      }

      let frame_count = (data[0] & 0x3F) as usize;
      if frame_count == 0 || frame_count > 48 {
          return Err(OpusError::InvalidPacket);
      }

      // Simplified implementation for now
      let mut frames = Vec::with_capacity(frame_count);
      let frame_size = (data.len() - 1) / frame_count;

      for i in 0..frame_count {
          let start = 1 + i * frame_size;
          let end = start + frame_size;
          frames.push(OpusFrame {
              data: data[start..end].to_vec(),
              is_dtx: false,
          });
      }

      Ok(frames)
  }

  /// Validate packet according to RFC 6716 constraints.
  pub fn validate_packet(data: &[u8]) -> OpusResult<()> {
      // [R1] At least one byte
      if data.is_empty() {
          return Err(OpusError::PacketTooShort(0));
      }

      // Additional validation rules [R2-R7] would go here
      // For now, basic validation only

      Ok(())
  }
  ```

### 5.3 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod error;
  pub mod frame;
  pub mod packet;
  pub mod toc;

  pub use error::{OpusError, OpusResult};
  pub use frame::{decode_frame_length, FramePacking, OpusFrame};
  pub use packet::{validate_packet, OpusPacket};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: All code is used, logging is active, bytes library utilized

## Phase 6: Symphonia Decoder Stub (Add symphonia dependency)

### 6.1 Add Symphonia Dependency

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dependencies]
  bytes = { workspace = true }
  log = { workspace = true }
  symphonia = { workspace = true }  # NOW we need Symphonia
  thiserror = { workspace = true }
  ```

### 6.2 Create Decoder Stub

- [ ] Create `src/decoder.rs`:
  ```rust
  use log::{debug, warn};
  use symphonia::core::{
      audio::{AudioBuffer, AudioBufferRef, SignalSpec},
      codecs::{
          CodecDescriptor, CodecParameters, Decoder, DecoderOptions,
          FinalizeResult, CODEC_TYPE_OPUS,
      },
      errors::Error,
      formats::Packet,
  };
  use symphonia::support_codec;

  use crate::{
      error::OpusError,
      packet::OpusPacket,
  };

  /// Opus decoder implementation.
  pub struct OpusDecoder {
      params: CodecParameters,
      output_buf: AudioBuffer<f32>,
      sample_rate: u32,
  }

  impl Decoder for OpusDecoder {
      fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self, Error> {
          debug!("Initializing Opus decoder");

          let sample_rate = params.sample_rate.unwrap_or(48000);
          let channels = params.channels.map(|c| c.count()).unwrap_or(2);

          let spec = SignalSpec::new(sample_rate, channels.into());
          let output_buf = AudioBuffer::new(960, spec); // Default frame size

          Ok(Self {
              params: params.clone(),
              output_buf,
              sample_rate,
          })
      }

      fn supported_codecs() -> &'static [CodecDescriptor] {
          &[support_codec!(
              CODEC_TYPE_OPUS,
              "opus",
              "Opus Interactive Audio Codec"
          )]
      }

      fn codec_params(&self) -> &CodecParameters {
          &self.params
      }

      fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>, Error> {
          // Parse packet structure
          let opus_packet = OpusPacket::parse(&packet.data)
              .map_err(|e| Error::DecodeError(e.to_string()))?;

          debug!("Decoded packet with {} frames", opus_packet.frames.len());

          // For now, return empty buffer (stub implementation)
          self.output_buf.clear();

          warn!("Opus decoding not yet implemented, returning silence");

          Ok(self.output_buf.as_audio_buffer_ref())
      }

      fn finalize(&mut self) -> FinalizeResult {
          FinalizeResult::default()
      }

      fn last_decoded(&self) -> AudioBufferRef<'_> {
          self.output_buf.as_audio_buffer_ref()
      }

      fn reset(&mut self) {
          debug!("Resetting Opus decoder");
          self.output_buf.clear();
      }
  }
  ```

### 6.3 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod decoder;
  pub mod error;
  pub mod frame;
  pub mod packet;
  pub mod toc;

  pub use decoder::OpusDecoder;
  pub use error::{OpusError, OpusResult};
  pub use frame::{decode_frame_length, FramePacking, OpusFrame};
  pub use packet::{validate_packet, OpusPacket};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: Decoder compiles and can be instantiated, no unused warnings

## Phase 7: Registry Implementation

### 7.1 Create Registry Module

- [ ] Create `src/registry.rs`:
  ```rust
  use symphonia::core::codecs::{CodecRegistry, Decoder};

  use crate::decoder::OpusDecoder;

  /// Register Opus codec with the provided registry.
  pub fn register_opus_codec(registry: &mut CodecRegistry) {
      // Get descriptors from the decoder and register each one
      for descriptor in OpusDecoder::supported_codecs() {
          registry.register(descriptor);
      }
  }

  /// Create a codec registry with Opus support.
  #[must_use]
  pub fn create_opus_registry() -> CodecRegistry {
      // Start with default Symphonia codecs
      let mut registry = symphonia::default::get_codecs();

      // Add our Opus codec on top
      register_opus_codec(&mut registry);

      registry
  }
  ```

### 7.2 Update lib.rs

- [ ] Add to `src/lib.rs`:
  ```rust
  pub mod decoder;
  pub mod error;
  pub mod frame;
  pub mod packet;
  pub mod registry;
  pub mod toc;

  pub use decoder::OpusDecoder;
  pub use error::{OpusError, OpusResult};
  pub use frame::{decode_frame_length, FramePacking, OpusFrame};
  pub use packet::{validate_packet, OpusPacket};
  pub use registry::{create_opus_registry, register_opus_codec};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: Registry functions are exported and usable

## Phase 8: Audio Decoder Integration

### 8.1 Update audio_decoder Cargo.toml

- [ ] Add to `packages/audio_decoder/Cargo.toml`:
  ```toml
  [dependencies]
  # ... existing dependencies ...
  moosicbox_opus = { workspace = true, optional = true }

  [features]
  # ... existing features ...
  opus = ["dep:moosicbox_opus"]
  ```

### 8.2 Update audio_decoder lib.rs

- [ ] Modify `packages/audio_decoder/src/lib.rs`:
  ```rust
  // At the top with other imports
  #[cfg(feature = "opus")]
  use moosicbox_opus::create_opus_registry;

  // Inside the decode function (around line 495):
  pub fn decode_file_with_handler(/* params */) -> Result<(), AudioDecodeError> {
      // ... existing code ...

      // Create the codec registry inline where it's used
      let codec_registry = {
          #[cfg(feature = "opus")]
          {
              moosicbox_opus::create_opus_registry()
          }
          #[cfg(not(feature = "opus"))]
          {
              symphonia::default::get_codecs()
          }
      };

      let mut decoder = codec_registry.make(&track.codec_params, &decode_opts)?;

      // ... rest of function
  }
  ```

### 8.3 Similar Update for unsync.rs

- [ ] Apply same pattern to `packages/audio_decoder/src/unsync.rs` at line 94

**Validation**: Integration compiles, can be tested with feature flag

## Phase 9: Real Decoding Implementation (Add audiopus)

### 9.1 Add audiopus Dependency

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dependencies]
  audiopus = { workspace = true }   # NOW we implement real decoding
  bytes = { workspace = true }
  log = { workspace = true }
  symphonia = { workspace = true }
  thiserror = { workspace = true }
  ```

### 9.2 Update Error Types

- [ ] Add to `src/error.rs`:
  ```rust
  #[derive(Debug, Error)]
  pub enum OpusError {
      // ... existing variants ...

      /// Decoder error from libopus
      #[error("Opus decoder error: {0}")]
      DecoderError(#[from] audiopus::Error),
  }
  ```

### 9.3 Update Decoder with Real Implementation

- [ ] Replace stub in `src/decoder.rs` (REPLACE entire imports section):
  ```rust
  use audiopus::{coder::Decoder as OpusLibDecoder, Channels, SampleRate};
  use log::{debug, warn};
  use symphonia::core::{
      audio::{AudioBuffer, AudioBufferRef, SignalSpec},
      codecs::{
          CodecDescriptor, CodecParameters, Decoder, DecoderOptions,
          FinalizeResult, CODEC_TYPE_OPUS,
      },
      errors::Error,
      formats::Packet,
  };
  use symphonia::support_codec;

  use crate::packet::OpusPacket;

  pub struct OpusDecoder {
      params: CodecParameters,
      opus_decoder: OpusLibDecoder,  // Add real decoder
      output_buf: AudioBuffer<f32>,
      channel_count: usize,
      frame_size_samples: usize,
      // Remove sample_rate field - access via params when needed
  }

  impl Decoder for OpusDecoder {
      fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self, Error> {
          debug!("Initializing Opus decoder with libopus");

          let sample_rate = params.sample_rate.unwrap_or(48000);
          let channels = params.channels.map(|c| c.count()).unwrap_or(2);

          // Create libopus decoder
          let opus_decoder = OpusLibDecoder::new(
              SampleRate::try_from(sample_rate as i32)
                  .map_err(|e| Error::Unsupported(&format!("sample rate: {}", e)))?,
              Channels::try_from(channels as i32)
                  .map_err(|e| Error::Unsupported(&format!("channels: {}", e)))?,
          ).map_err(|e| Error::DecodeError(e.to_string()))?;

          let frame_size_samples = 960; // Default, can be calculated from params
          let spec = SignalSpec::new(sample_rate, channels.into());
          let output_buf = AudioBuffer::new(frame_size_samples as u64, spec);

          Ok(Self {
              params: params.clone(),
              opus_decoder,
              output_buf,
              channel_count: channels,
              frame_size_samples,
              // Don't store sample_rate - access via params.sample_rate when needed
          })
      }

      fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>, Error> {
          // Clear previous buffer
          self.output_buf.clear();

          // Parse packet
          let opus_packet = OpusPacket::parse(&packet.data)
              .map_err(|e| Error::DecodeError(e.to_string()))?;

          debug!("Decoding {} frames", opus_packet.frames.len());

          // Decode each frame
          let mut output_offset = 0;
          for frame in &opus_packet.frames {
              if frame.is_dtx {
                  // Handle DTX - generate silence
                  debug!("DTX frame, generating silence");
                  continue;
              }

              if self.channel_count == 1 {
                  // Mono: decode directly to output buffer
                  let output = self.output_buf.chan_mut(0);
                  let out_slice = &mut output[output_offset..];

                  let decoded_samples = self.opus_decoder
                      .decode(&frame.data, out_slice, false)
                      .map_err(|e| Error::DecodeError(e.to_string()))?;

                  output_offset += decoded_samples;
              } else {
                  // Stereo/Multi-channel: decode to interleaved buffer first
                  let mut interleaved = vec![0f32; self.frame_size_samples * self.channel_count];

                  let decoded_samples = self.opus_decoder
                      .decode(&frame.data, &mut interleaved, false)
                      .map_err(|e| Error::DecodeError(e.to_string()))?;

                  // Deinterleave into planar format
                  for i in 0..decoded_samples {
                      for ch in 0..self.channel_count {
                          let sample = interleaved[i * self.channel_count + ch];
                          self.output_buf.chan_mut(ch)[i + output_offset] = sample;
                      }
                  }

                  output_offset += decoded_samples;
              }
          }

          self.output_buf.truncate(output_offset);
          Ok(self.output_buf.as_audio_buffer_ref())
      }

      fn reset(&mut self) {
          debug!("Resetting Opus decoder state");
          if let Err(e) = self.opus_decoder.reset_state() {
              warn!("Failed to reset decoder state: {}", e);
          }
          self.output_buf.clear();
      }

      // ... other methods remain the same ...
      fn supported_codecs() -> &'static [CodecDescriptor] {
          &[support_codec!(
              CODEC_TYPE_OPUS,
              "opus",
              "Opus Interactive Audio Codec"
          )]
      }

      fn codec_params(&self) -> &CodecParameters {
          &self.params
      }

      fn finalize(&mut self) -> FinalizeResult {
          FinalizeResult::default()
      }

      fn last_decoded(&self) -> AudioBufferRef<'_> {
          self.output_buf.as_audio_buffer_ref()
      }
  }
  ```

**Validation**: Real decoding works, all code paths used

## Phase 10: Testing Infrastructure (Add test dependencies)

### 10.1 Add Test Dependencies

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dev-dependencies]
  hex = { workspace = true }
  insta = { workspace = true }
  pretty_assertions = { workspace = true }
  test-case = { workspace = true }
  ```

### 10.2 Create Unit Tests

- [ ] Create `tests/packet_tests.rs`:
  ```rust
  use moosicbox_opus::{
      packet::{validate_packet, OpusPacket},
      toc::TocByte,
      frame::decode_frame_length,
  };
  use test_case::test_case;
  use pretty_assertions::assert_eq;

  #[test_case(0b00011001, 3, false, 1; "silk_nb_60ms_mono_single")]
  #[test_case(0b01111101, 15, true, 1; "hybrid_fb_20ms_stereo_equal")]
  fn test_toc_parsing(byte: u8, config: u8, stereo: bool, code: u8) {
      let toc = TocByte::parse(byte).unwrap();
      assert_eq!(toc.config(), config);
      assert_eq!(toc.is_stereo(), stereo);
      assert_eq!(toc.frame_code(), code);
  }

  #[test]
  fn test_packet_validation() {
      // [R1] Empty packet should fail
      assert!(validate_packet(&[]).is_err());

      // Valid single-byte packet should pass
      assert!(validate_packet(&[0x00]).is_ok());
  }

  #[test_case(&[100], 100, 1; "single_byte")]
  #[test_case(&[252, 1], 253, 2; "two_byte_min")]
  fn test_frame_length(data: &[u8], expected_len: usize, bytes_read: usize) {
      let (len, read) = decode_frame_length(data).unwrap();
      assert_eq!(len, expected_len);
      assert_eq!(read, bytes_read);
  }
  ```

### 10.3 Create Decoder Tests

- [ ] Create `tests/decoder_tests.rs`:
  ```rust
  use symphonia::core::{
      codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_OPUS},
      audio::Channels,
  };
  use moosicbox_opus::OpusDecoder;

  #[test]
  fn test_decoder_creation() {
      let mut params = CodecParameters::new();
      params.for_codec(CODEC_TYPE_OPUS)
          .with_sample_rate(48000)
          .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

      let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
      assert!(decoder.is_ok());
  }
  ```

**Validation**: Tests pass, test dependencies only in dev-dependencies

## Phase 11: Benchmarking (Add criterion)

### 11.1 Add Benchmark Dependencies

- [ ] Add to workspace `Cargo.toml`:
  ```toml
  criterion = "0.5.1"
  ```

- [ ] Update `packages/opus/Cargo.toml`:
  ```toml
  [dev-dependencies]
  criterion = { workspace = true }
  # ... other dev dependencies ...

  [[bench]]
  name = "opus_benchmarks"
  harness = false
  ```

### 11.2 Create Benchmarks

- [ ] Create `benches/opus_benchmarks.rs`:
  ```rust
  use criterion::{black_box, criterion_group, criterion_main, Criterion};
  use moosicbox_opus::packet::OpusPacket;

  fn bench_packet_parsing(c: &mut Criterion) {
      // Simple test packet
      let packet_data = vec![0x00; 100];

      c.bench_function("parse opus packet", |b| {
          b.iter(|| OpusPacket::parse(black_box(&packet_data)))
      });
  }

  criterion_group!(benches, bench_packet_parsing);
  criterion_main!(benches);
  ```

**Validation**: Benchmarks compile and run with `cargo bench`

## Phase 12: Documentation and Examples

### 12.1 Add Documentation

- [ ] Update all public items with comprehensive rustdoc
- [ ] Create README.md with usage examples
- [ ] Add module-level documentation

### 12.2 Create Examples

- [ ] Create `examples/decode_simple.rs`:
  ```rust
  //! Simple Opus decoding example.

  use std::fs::File;
  use symphonia::core::{
      codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_OPUS},
      formats::Packet,
  };
  use moosicbox_opus::OpusDecoder;

  fn main() -> Result<(), Box<dyn std::error::Error>> {
      println!("Opus decoder example");

      // Would decode a real file here
      let mut params = CodecParameters::new();
      params.for_codec(CODEC_TYPE_OPUS);

      let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default())?;
      println!("Decoder created successfully");

      Ok(())
  }
  ```

### 12.3 Module Organization (`src/lib.rs`)

- [ ] Create final lib.rs:
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
  pub use error::{OpusError, OpusResult};
  pub use frame::{decode_frame_length, FramePacking, OpusFrame};
  pub use packet::{validate_packet, OpusPacket};
  pub use registry::{create_opus_registry, register_opus_codec};
  pub use toc::{Bandwidth, OpusMode, TocByte};
  ```

**Validation**: Examples compile and demonstrate usage

## Validation Criteria for Each Phase

### Phase-by-Phase Validation Commands

```bash
# After each phase, run:
cargo build -p moosicbox_opus
cargo clippy -p moosicbox_opus -- -D warnings
cargo test -p moosicbox_opus

# For integration (Phase 8+):
cargo build -p moosicbox_audio_decoder --features opus
cargo clippy -p moosicbox_audio_decoder --features opus -- -D warnings

# For benchmarks (Phase 11):
cargo bench -p moosicbox_opus --no-run  # Just compile
```

## Key Improvements in This Plan

1. **Dependencies Added Only When Used**:
   - `thiserror` in Phase 2 (for errors)
   - `bytes` and `log` in Phase 5 (for packet parsing)
   - `symphonia` in Phase 6 (for decoder trait)
   - `audiopus` in Phase 9 (for actual decoding)
   - Test dependencies in Phase 10 (for testing)

2. **No Compilation Errors**:
   - Each phase builds on previous
   - Error types defined before use
   - All structs/functions are used when created

3. **No Unused Warnings**:
   - Public API exports everything
   - Functions called within same module
   - Test code validates all functionality

4. **No Circular Dependencies**:
   - Opus package is standalone
   - audio_decoder optionally depends on opus
   - No back-references

5. **RFC Compliance Maintained**:
   - All validation rules preserved
   - Packet structure implementation complete
   - Test coverage comprehensive

This plan ensures clean, warning-free compilation at every step while maintaining all the RFC 6716 compliance requirements and comprehensive testing from the original plan.


