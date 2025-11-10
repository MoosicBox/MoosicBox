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
# All features enabled by default
[dependencies]
moosicbox_audio_encoder = { path = "../audio_encoder" }

# Or disable default features and enable specific formats only
moosicbox_audio_encoder = {
    path = "../audio_encoder",
    default-features = false,
    features = ["mp3", "flac"]
}
```

## Usage

### Basic Encode Info

```rust
use moosicbox_audio_encoder::EncodeInfo;

fn handle_encode_result(info: EncodeInfo) {
    println!("Output size: {} bytes", info.output_size);
    println!("Input consumed: {} samples", info.input_consumed);
}
```

### Format-Specific Encoding

```rust
// AAC encoding (requires "aac" feature)
#[cfg(feature = "aac")]
use moosicbox_audio_encoder::aac::{encoder_aac, encode_aac};

// FLAC encoding (requires "flac" feature)
#[cfg(feature = "flac")]
use moosicbox_audio_encoder::flac::{encoder_flac, encode_flac};

// MP3 encoding (requires "mp3" feature)
#[cfg(feature = "mp3")]
use moosicbox_audio_encoder::mp3::{encoder_mp3, encode_mp3};

// Opus encoding (requires "opus" feature)
#[cfg(feature = "opus")]
use moosicbox_audio_encoder::opus::{encoder_opus, encode_opus_float};

fn encode_audio() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "mp3")]
    {
        // Create and use MP3 encoder
        let mut encoder = encoder_mp3()?;
        let input: Vec<i16> = vec![0; 1024]; // Your PCM data here
        let (output, info) = encode_mp3(&mut encoder, &input)?;
        println!("Encoded {} bytes", info.output_size);
    }

    #[cfg(feature = "aac")]
    {
        // Create and use AAC encoder
        let encoder = encoder_aac()?;
        let input: Vec<i16> = vec![0; 1024]; // Your PCM data here
        let mut output = vec![0u8; 4096];
        let info = encode_aac(&encoder, &input, &mut output)?;
        println!("Encoded {} bytes", info.output_size);
    }

    #[cfg(feature = "flac")]
    {
        // Create and use FLAC encoder
        let mut encoder = encoder_flac()?;
        let input: Vec<i32> = vec![0; 1024]; // Your PCM data here
        let mut output = vec![0u8; 4096];
        let info = encode_flac(&mut encoder, &input, &mut output)?;
        println!("Encoded {} bytes", info.output_size);
    }

    #[cfg(feature = "opus")]
    {
        // Create and use Opus encoder
        let mut encoder = encoder_opus()?;
        let input: Vec<f32> = vec![0.0; 1024]; // Your PCM data here
        let mut output = vec![0u8; 4096];
        let info = encode_opus_float(&mut encoder, &input, &mut output)?;
        println!("Encoded {} bytes", info.output_size);
    }

    Ok(())
}
```

## Implementation Notes

- The package provides a minimal core with feature-gated format modules
- Each audio format is implemented in its own module behind a feature flag
- The `EncodeInfo` struct provides standardized encoding result information
- Actual encoding functionality is contained within the format-specific modules
- All features are enabled by default, but can be disabled if needed

### Module APIs

Each encoder module provides consistent functions:

- **AAC** (`aac` feature): `encoder_aac()` creates encoder, `encode_aac(encoder, input, buf)` encodes i16 PCM data
- **FLAC** (`flac` feature): `encoder_flac()` creates encoder, `encode_flac(encoder, input, buf)` encodes i32 PCM data
- **MP3** (`mp3` feature): `encoder_mp3()` creates encoder, `encode_mp3(encoder, input)` encodes i16 PCM data and returns output buffer
- **Opus** (`opus` feature): `encoder_opus()` creates encoder, `encode_opus_float(encoder, input, output)` encodes f32 PCM data, also includes `encode_audiopus()` and OGG container utilities

## Features

- **Default**: All encoding formats are enabled by default (`aac`, `flac`, `mp3`, `opus`)
- **`aac`**: Enables AAC encoding module via fdk-aac
- **`flac`**: Enables FLAC encoding module via flacenc
- **`mp3`**: Enables MP3 encoding module via mp3lame-encoder
- **`opus`**: Enables Opus encoding module via audiopus/opus, includes OGG container support

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
use moosicbox_audio_encoder::mp3::{encoder_mp3, encode_mp3};

#[cfg(feature = "flac")]
use moosicbox_audio_encoder::flac::{encoder_flac, encode_flac};
```

This design allows consumers to include only the encoding formats they need by disabling default features, reducing binary size and dependencies.
