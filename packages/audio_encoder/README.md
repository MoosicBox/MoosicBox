# MoosicBox Audio Encoder

Basic audio encoding utilities with feature-gated support for multiple formats.

## Overview

The MoosicBox Audio Encoder package provides:

- **Feature-Gated Modules**: Optional encoding support for different audio formats
- **Format-Specific Encoders**: Separate modules for different encoding formats
- **Minimal Core**: Lightweight base with optional format extensions
- **Encode Info Structure**: Basic encoding result information

## Current Implementation

### Core Components
- **EncodeInfo**: Structure containing encoding results (output size, input consumed)
- **Feature-Gated Modules**: Format-specific encoding implementations

### Available Features
- **`aac`**: Advanced Audio Coding encoder module
- **`flac`**: Free Lossless Audio Codec encoder module
- **`mp3`**: MPEG Layer III encoder module
- **`opus`**: Opus codec encoder module

## Installation

### Cargo Dependencies

```toml
[dependencies]
moosicbox_audio_encoder = { path = "../audio_encoder" }

# Enable specific formats
moosicbox_audio_encoder = {
    path = "../audio_encoder",
    features = ["mp3", "flac", "aac", "opus"]
}
```

## Usage

### Basic Encode Info

```rust
use moosicbox_audio_encoder::EncodeInfo;

fn handle_encode_result(info: EncodeInfo) {
    println!("Output size: {} bytes", info.output_size);
    println!("Input consumed: {} bytes", info.input_consumed);
}
```

### Format-Specific Encoding

```rust
// AAC encoding (requires "aac" feature)
#[cfg(feature = "aac")]
use moosicbox_audio_encoder::aac;

// FLAC encoding (requires "flac" feature)
#[cfg(feature = "flac")]
use moosicbox_audio_encoder::flac;

// MP3 encoding (requires "mp3" feature)
#[cfg(feature = "mp3")]
use moosicbox_audio_encoder::mp3;

// Opus encoding (requires "opus" feature)
#[cfg(feature = "opus")]
use moosicbox_audio_encoder::opus;

async fn encode_audio() -> Result<(), Box<dyn std::error::Error>> {
    // Encoding functionality depends on enabled features
    // and implementations in the respective modules

    #[cfg(feature = "mp3")]
    {
        // Use MP3 encoder from mp3 module
        // (implementation details depend on the mp3 module)
    }

    #[cfg(feature = "flac")]
    {
        // Use FLAC encoder from flac module
        // (implementation details depend on the flac module)
    }

    Ok(())
}
```

## Implementation Notes

- The package provides a minimal core with feature-gated format modules
- Each audio format is implemented in its own module behind a feature flag
- The `EncodeInfo` struct provides standardized encoding result information
- Actual encoding functionality is contained within the format-specific modules
- Features must be explicitly enabled to access format encoders

## Features

- **Default**: Includes only the core `EncodeInfo` structure
- **`aac`**: Enables AAC encoding module
- **`flac`**: Enables FLAC encoding module
- **`mp3`**: Enables MP3 encoding module
- **`opus`**: Enables Opus encoding module

## Development Status

This package currently provides:

1. **Core Structure**: `EncodeInfo` for encoding results
2. **Module Framework**: Feature-gated organization for different encoders
3. **Extensible Design**: Easy addition of new encoding formats

The actual encoding implementations are contained within the feature-gated modules. Check the individual module documentation for specific encoding capabilities and APIs.

## Usage Patterns

```rust
// Always available - core types
use moosicbox_audio_encoder::EncodeInfo;

// Feature-gated - format-specific encoders
#[cfg(feature = "mp3")]
use moosicbox_audio_encoder::mp3::*;

#[cfg(feature = "flac")]
use moosicbox_audio_encoder::flac::*;
```

This design allows consumers to include only the encoding formats they need, reducing binary size and dependencies.
