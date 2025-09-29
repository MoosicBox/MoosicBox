# Opus Codec Architecture

## Overview

The Opus codec integration follows Symphonia's trait-based architecture to provide native Opus decoding capability within MoosicBox's audio pipeline. The implementation adheres strictly to RFC 6716 specification while maintaining seamless integration with existing audio processing components.

## Design Goals

- **RFC 6716 Compliance**: Bit-exact compliance with Opus specification including all validation rules [R1-R7]
- **Performance**: Efficient decoding leveraging libopus reference implementation
- **Maintainability**: Modular design with clear separation of concerns and comprehensive error handling
- **Extensibility**: Support for all Opus modes (SILK/CELT/Hybrid) and container formats (Ogg/WebM/Matroska)

## High-Level Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Audio Files   │───▶│  Format Demuxer  │───▶│  Codec Registry │
│  (.ogg, .webm,  │    │   (Symphonia)    │    │   (Custom)      │
│   .mkv, .mka)   │    │                  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  Audio Output   │◀───│  Audio Pipeline  │◀───│  Opus Decoder   │
│   (PCM f32)     │    │  (MoosicBox)     │    │   (RFC 6716)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## Detailed Component Architecture

### Package Structure (`moosicbox_opus`)

```
packages/opus/
├── src/
│   ├── lib.rs           # Public API and module exports
│   ├── decoder.rs       # OpusDecoder (Symphonia trait impl)
│   ├── packet.rs        # Packet parsing (RFC Section 3)
│   ├── toc.rs           # TOC byte parsing (Section 3.1)
│   ├── frame.rs         # Frame packing (Section 3.2)
│   ├── error.rs         # Comprehensive error types
│   └── registry.rs      # Custom codec registry
└── tests/
    ├── packet_tests.rs  # RFC compliance tests
    ├── decoder_tests.rs # Integration tests
    └── fixtures/        # Test vectors and sample files
```

## Core Components

### 1. Opus Decoder (`decoder.rs`)

**Primary Component**: Implements Symphonia's `Decoder` trait

```rust
pub struct OpusDecoder {
    params: CodecParameters,
    opus_decoder: audiopus::coder::Decoder,  // libopus wrapper
    output_buf: AudioBuffer<f32>,           // Pre-allocated output
    sample_rate: u32,                       // 8-48 kHz
    channel_count: usize,                   // 1-255 channels
    frame_size_samples: usize,              // Calculated from config
}
```

**Responsibilities**:
- Opus packet parsing via `OpusPacket::parse()`
- Frame-by-frame decoding through libopus
- Audio buffer management with proper channel mapping
- Packet loss concealment (PLC) and error recovery
- DTX (discontinuous transmission) handling

**Key Methods**:
- `try_new()`: Initialize decoder with codec parameters
- `decode()`: Process Opus packet → PCM audio samples
- `reset()`: Reset decoder state (post-seek)
- `supported_codecs()`: Register codec descriptor

### 2. Packet Parser (`packet.rs`)

**RFC Section 3 Implementation**

```rust
pub struct OpusPacket {
    toc: TocByte,           // Table of contents
    frames: Vec<OpusFrame>, // 1-48 frames per packet
    padding: Vec<u8>,       // Optional padding
}
```

**Frame Packing Support**:
- **Code 0**: Single frame (most common)
- **Code 1**: Two frames, equal size
- **Code 2**: Two frames, different sizes
- **Code 3**: 1-48 frames with explicit count

**Validation Rules [R1-R7]**:
- [R1] Minimum packet size (1 byte)
- [R2] Maximum frame size (1275 bytes)
- [R3] Code 1 odd total length constraint
- [R4] Code 2 frame length validation
- [R5] Code 3 frame count limits (≤120ms total)
- [R6] CBR padding constraints
- [R7] VBR header size validation

### 3. TOC Byte Parser (`toc.rs`)

**RFC Section 3.1 Implementation**

```rust
pub struct TocByte {
    config: u8,      // Bits 0-4: Configuration (0-31)
    stereo: bool,    // Bit 5: Channel flag
    frame_code: u8,  // Bits 6-7: Frame count code
}
```

**Configuration Mapping** (RFC Table 2):
- **0-3**: SILK NB (10/20/40/60ms)
- **4-7**: SILK MB (10/20/40/60ms)
- **8-11**: SILK WB (10/20/40/60ms)
- **12-13**: Hybrid SWB (10/20ms)
- **14-15**: Hybrid FB (10/20ms)
- **16-19**: CELT NB (2.5/5/10/20ms)
- **20-23**: CELT WB (2.5/5/10/20ms)
- **24-27**: CELT SWB (2.5/5/10/20ms)
- **28-31**: CELT FB (2.5/5/10/20ms)

### 4. Frame Processing (`frame.rs`)

**Frame Length Encoding** (RFC Section 3.2.1):
- **0**: DTX/silence frame
- **1-251**: Direct byte length
- **252-255**: Two-byte encoding (max 1275 bytes)

**Multi-frame Handling**:
- CBR: Equal-sized frames
- VBR: Length-prefixed frames
- Padding: Optional trailing bytes

### 5. Custom Codec Registry (`registry.rs`)

**Integration Strategy**: Extend Symphonia without core modifications

```rust
pub fn create_opus_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    register_opus_codec(&mut registry);
    symphonia::default::register_enabled_codecs(&mut registry);
    registry
}
```

**Registration**: Replace default codec registry at specific integration points

## Integration Points

### MoosicBox Audio Decoder Integration

**Primary Integration**: `/packages/audio_decoder/src/lib.rs`

```rust
#[cfg(feature = "opus")]
let codec_registry = moosicbox_opus::create_opus_registry();

#[cfg(not(feature = "opus"))]
let codec_registry = symphonia::default::get_codecs();

let mut decoder = codec_registry.make(&track.codec_params, &decode_opts)?;
```

**Feature Flag Control**: `opus = ["dep:moosicbox_opus"]`

### Container Format Support

| Container | Status | Notes |
|-----------|--------|-------|
| **Ogg** | Primary | Native Opus container format |
| **WebM** | Secondary | Web streaming compatibility |
| **Matroska** | Secondary | MKV/MKA file support |

**Codec Parameters**: Extracted by Symphonia demuxers
- Sample rate detection (8-48 kHz)
- Channel count (1-255 channels)
- Opus-specific headers (OpusHead)
- Delay/padding information

## Data Flow

### Decoding Pipeline

1. **Packet Reception**: Raw Opus packet from container
2. **Packet Parsing**: TOC extraction → frame identification
3. **Frame Validation**: Length checks → RFC compliance
4. **libopus Decoding**: Native decoding → PCM samples
5. **Buffer Management**: Channel deinterleaving → format conversion
6. **Output**: Symphonia AudioBufferRef<f32>

### Error Handling Flow

```
Packet → Validation → Parse Error?
                         ↓ No
Frame → Decode → libopus Error?
                    ↓ No        ↓ Yes
                  PCM ← PLC/Silence
```

## Performance Characteristics

### Memory Management
- **Pre-allocated buffers**: Minimize allocation overhead
- **Zero-copy parsing**: Direct slice references where possible
- **Reusable structures**: Frame data structure pooling

### CPU Optimization
- **libopus acceleration**: SIMD optimizations in reference implementation
- **Minimal validation overhead**: Cached configuration lookups
- **Efficient channel mapping**: Optimized interleaving/deinterleaving

### Benchmarking Targets
- **Packet parsing**: <1μs per packet
- **Frame decoding**: Real-time factor <0.1
- **Memory usage**: <1MB steady state

## Testing Strategy

### Unit Testing (95% Coverage Target)
- **Packet parsing**: All RFC test vectors + malformed inputs
- **TOC byte handling**: All 32 configurations
- **Frame length encoding**: Boundary conditions (0, 251, 1275)
- **Validation rules**: Each RFC constraint [R1-R7]

### Integration Testing
- **Container formats**: Real Ogg/WebM/MKV files
- **Codec modes**: SILK/CELT/Hybrid samples
- **Error scenarios**: Corrupted packets, PLC testing
- **Memory profiling**: Leak detection, allocation patterns

### RFC Compliance Testing
- **Appendix A vectors**: Reference decoder output comparison
- **Interoperability**: opus-tools compatibility verification
- **Bit-exact validation**: Sample-level output matching

### Performance Testing
- **Criterion benchmarks**: Decode speed measurement
- **Memory profiling**: Peak usage analysis
- **Stress testing**: Extended playback scenarios

## Security Considerations

### Input Validation
- **Packet size limits**: Prevent buffer overflow
- **Frame count bounds**: Limit memory allocation
- **Length validation**: Prevent integer overflow

### Memory Safety
- **Safe Rust patterns**: No unsafe code blocks
- **Bounds checking**: All array/slice access validated
- **Error propagation**: Graceful failure handling

## Monitoring and Observability

### Logging Strategy
- **Debug**: Packet structure details
- **Info**: Decoder state changes
- **Warn**: Recoverable errors (PLC activation)
- **Error**: Unrecoverable failures

### Metrics Collection
- **Decode performance**: Frame processing time
- **Error rates**: Packet loss/corruption frequency
- **Resource usage**: Memory/CPU utilization
