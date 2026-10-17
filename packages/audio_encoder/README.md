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

## Explicit Native Ogg Opus Encoding

Enable `opus-native` with default features disabled for a pure-Rust encoder and
standalone Ogg framing. `opus_stream::OpusEncodeConfig` separates backend from
`OpusEncodeMode`: `Default` preserves native CELT or libopus automatic selection;
`Celt`, `Silk`, and `Hybrid` force the native constructor and reject a Libopus
backend. There is no hidden fallback or automatic native mode adaptation.

```rust,ignore
use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode, OpusStreamEncoder,
};
let config = OpusEncodeConfig {
    backend: OpusEncodeBackend::Native,
    mode: OpusEncodeMode::Silk,
    sample_rate: 16_000,
    channels: 2,
    bitrate: 128_000,
    frame_size: 320,
};
let mut encoder = OpusStreamEncoder::new(Vec::new(), config)?;
encoder.write(&interleaved_pcm)?;
let ogg_bytes = encoder.finish()?.expect("first finish returns the sink");
```

Supported native construction boundaries (not a claim of full Opus encoder parity):

| Mode   | Input rates       | Frame durations | Bitrate                                                  |
| ------ | ----------------- | --------------- | -------------------------------------------------------- |
| CELT   | 8/12/16/24/48 kHz | 2.5/5/10/20 ms  | Minimum 128/64/32/32 kbps respectively; maximum 256 kbps |
| SILK   | 8/12/16 kHz       | 10/20 ms        | Mono 48–128 kbps; stereo 96–128 kbps                     |
| Hybrid | 48 kHz            | 10/20 ms        | 128–256 kbps                                             |

Mono and stereo are supported. Lower-bitrate quantization is not currently
supported: rejected configurations fail before writing Ogg headers. Encoder
lookahead comes from the selected codec getter, scales to the Ogg 48-kHz clock,
and is removed along with partial-frame padding using the actual input length.
SILK currently uses unvoiced predictive coding; explicit selection does not imply
adaptive mode or voiced-analysis support.

The same configuration is accepted by the audio-output `SelectedOpusEncoder`,
player `SignalChainStep::with_opus_encoder`, and files
`get_audio_bytes_with_opus_encoder`. The explicit files API always re-encodes
(including Opus sources) and bypasses shared encoded-byte caching. Codec and I/O
errors are propagated; repeated successful finalization emits no additional EOS.
Finite seek preroll guarantees the requested sample cutoff, not identical
prediction history to decoding from stream origin.

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
- **Opus** (`opus` feature): `encoder_opus()` creates encoder, `encode_opus_float(encoder, input, output)` encodes f32 PCM data, and `encode_audiopus(samples)` produces length-prefixed Opus packets

Opus/Ogg helpers exported by the `opus` module:

- `OpusWrite::new(path)` creates an Ogg/Opus stream writer that implements `std::io::Write`
- `write_ogg(file, content)` writes one Ogg packet with end-of-stream marker
- `read_write_ogg(read, write)` copies packets from one Ogg stream to another

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
