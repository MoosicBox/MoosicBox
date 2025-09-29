# Opus Codec Implementation Plan

## Executive Summary

This document outlines the phased implementation of Opus codec support in MoosicBox. The implementation leverages Symphonia's trait system to provide native Opus decoding without requiring modifications to the core Symphonia library.

**Status**: Planning Phase
**Target**: Full Opus codec support integrated with MoosicBox audio pipeline
**Timeline**: 6 phases, estimated 3-4 weeks total development time

## Implementation Phases

### Phase 1: Project Setup and Dependencies

**Goal**: Establish the foundation for Opus codec development

- [ ] Create `moosicbox-opus` crate in `/packages/opus/`
- [ ] Configure Cargo.toml with Symphonia dependencies
- [ ] Add libopus-sys or opus-rs binding dependencies
- [ ] Set up basic project structure with lib.rs
- [ ] Create initial CI configuration for the new crate
- [ ] Establish test data directory with sample Opus files

**Deliverables**: Buildable Rust crate with proper dependency management

### Phase 2: Opus Packet Structure Implementation

**Goal**: Implement basic Opus packet parsing according to RFC 6716

- [ ] Implement Opus packet header parsing (RFC 6716 Section 3.1)
- [ ] Create packet validation functions (RFC 6716 Section 3.1.1)
- [ ] Implement TOC (Table of Contents) parsing (RFC 6716 Section 3.1)
- [ ] Add frame structure identification (SILK, CELT, or Hybrid modes)
- [ ] Create packet length validation and boundary checking
- [ ] Implement multi-frame packet handling
- [ ] Add comprehensive unit tests for packet parsing

**Deliverables**: Robust Opus packet parser with full test coverage

### Phase 3: Symphonia Decoder Trait Implementation

**Goal**: Create the core Opus decoder implementing Symphonia's Decoder trait

- [ ] Implement `Decoder` trait skeleton with required methods
- [ ] Create `OpusDecoder` struct with necessary state management
- [ ] Implement `try_new()` method for decoder initialization
- [ ] Implement `reset()` method for stream reset functionality
- [ ] Create codec parameters parsing and validation
- [ ] Add channel configuration and mapping support
- [ ] Implement sample rate and frame size detection
- [ ] Create basic decode() method framework

**Deliverables**: Complete Decoder trait implementation (without actual decoding)

### Phase 4: Core Decoding Implementation

**Goal**: Implement the actual Opus audio decoding functionality

- [ ] Integrate libopus decoder initialization
- [ ] Implement frame decoding for SILK mode (RFC 6716 Section 4.2)
- [ ] Implement frame decoding for CELT mode (RFC 6716 Section 4.3)
- [ ] Implement hybrid mode decoding (RFC 6716 Section 4.4)
- [ ] Add proper sample format conversion (f32/i16/i32 support)
- [ ] Implement channel mapping and multichannel support
- [ ] Add error handling and recovery for corrupted packets
- [ ] Implement packet loss concealment (PLC)

**Deliverables**: Functional Opus decoder producing valid PCM output

### Phase 5: MoosicBox Integration

**Goal**: Integrate Opus decoder with MoosicBox's audio pipeline

- [ ] Create custom CodecRegistry in audio_decoder package
- [ ] Register Opus decoder with appropriate codec parameters
- [ ] Update audio_decoder/lib.rs to use custom registry
- [ ] Implement Opus format detection and initialization
- [ ] Add container format support (Ogg, WebM, Matroska)
- [ ] Test end-to-end audio playback with Opus files
- [ ] Validate integration with existing audio processing pipeline
- [ ] Add logging and debugging support for troubleshooting

**Deliverables**: Fully integrated Opus support in MoosicBox

### Phase 6: Testing, Optimization, and Documentation

**Goal**: Ensure production readiness and comprehensive validation

- [ ] Create comprehensive test suite with RFC 6716 test vectors
- [ ] Add integration tests for various Opus file formats
- [ ] Implement performance benchmarking and optimization
- [ ] Add memory usage profiling and optimization
- [ ] Create API documentation and usage examples
- [ ] Add error handling documentation and troubleshooting guide
- [ ] Validate against opus-tools reference implementation
- [ ] Performance comparison with other Symphonia codecs

**Deliverables**: Production-ready Opus codec with full documentation

## Design Decisions

### Dependency Strategy

**Decision**: Use libopus bindings rather than pure Rust implementation
**Rationale**: Leverages mature, optimized reference implementation while maintaining compatibility with Opus specification updates

### Integration Approach

**Decision**: Custom CodecRegistry rather than Symphonia core modifications
**Rationale**: Maintains upgrade compatibility with Symphonia while providing full codec functionality

### Container Support Priority

**Decision**: Implement Ogg first, then WebM/Matroska
**Rationale**: Ogg is the primary container for Opus files, providing maximum compatibility coverage

### Error Handling Strategy

**Decision**: Graceful degradation with packet loss concealment
**Rationale**: Maintains audio continuity during network streaming scenarios

## Test Scenarios

### Unit Testing

- Packet parsing validation with malformed inputs
- Decoder state management across stream resets
- Channel mapping verification for multi-channel streams
- Sample rate conversion accuracy testing

### Integration Testing

- End-to-end decode of various Opus file formats
- Container format compatibility (Ogg, WebM, MKV)
- Integration with MoosicBox playback pipeline
- Memory leak detection during extended playback

### Performance Testing

- Decode speed benchmarking against reference implementation
- Memory usage profiling during sustained operation
- CPU utilization measurement across different Opus configurations
- Comparison with existing Symphonia codec performance
