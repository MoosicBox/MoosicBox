# Opus Codec Architecture

## Overview

The Opus codec integration follows Symphonia's trait-based architecture to provide native Opus decoding capability within MoosicBox's audio pipeline. The design maintains separation of concerns while ensuring seamless integration with existing audio processing components.

## Design Goals

- **Specification Compliance**: Full adherence to RFC 6716 Opus codec specification
- **Performance**: Efficient decoding with minimal memory footprint and CPU overhead
- **Maintainability**: Clean trait implementation that follows Symphonia patterns
- **Extensibility**: Support for all Opus features including multiple streams and channel coupling

## System Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Audio Files   │───▶│  Format Demuxer  │───▶│  Codec Registry │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  Audio Output   │◀───│  Audio Pipeline  │◀───│  Opus Decoder   │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## Core Components

### Opus Decoder

Implements Symphonia's `Decoder` trait to handle:

- Opus packet parsing and validation (RFC 6716 Section 3)
- Frame structure decoding (RFC 6716 Section 3.1)
- Audio sample generation with proper channel mapping
- Error recovery and packet loss handling

### Codec Registry Integration

Custom `CodecRegistry` that extends the default Symphonia registry:

- Registers the Opus decoder with appropriate codec parameters
- Handles Opus-specific format detection and initialization
- Maintains compatibility with existing MoosicBox codec workflow

### Packet Processing Pipeline

- **Input**: Raw Opus packets from container format
- **Processing**: Packet validation, frame extraction, and decoding
- **Output**: PCM audio samples in Symphonia's standard format

## Integration Points

### MoosicBox Audio Decoder Package

Integration occurs at `/packages/audio_decoder/src/lib.rs:495` where the custom codec registry replaces the default Symphonia registry, enabling transparent Opus support across the application.

### Container Format Support

The decoder supports Opus streams within:

- Ogg containers (primary use case)
- WebM containers (for web compatibility)
- Matroska containers (MKV/MKA files)

## Testing Strategy

- **Unit Tests**: Individual component validation against RFC test vectors
- **Integration Tests**: End-to-end decoding of sample Opus files
- **Performance Tests**: Memory usage and decode speed benchmarking
- **Compatibility Tests**: Validation against reference encoder outputs
