# MoosicBox Audio Decoder

Audio decoding library for the MoosicBox ecosystem, built on top of the Symphonia media framework.

## Overview

The MoosicBox Audio Decoder package provides a wrapper around the [Symphonia](https://github.com/pdeljanov/Symphonia) media framework, enabling audio decoding for multiple formats within the MoosicBox ecosystem.

**Current Features:**

- Multi-format audio decoding via Symphonia
- Streaming support through media source streams
- Handler-based audio output management
- Filter support for audio processing
- Async and sync decoding interfaces
- Gapless playback support
- Basic seeking capabilities (via Symphonia)

## Supported Formats

Format support is provided through Symphonia and optional feature flags:

- **MP3**: Via `mp3` feature flag
- **FLAC**: Via `flac` feature flag
- **AAC**: Via `aac` feature flag
- **Opus**: Via `opus` feature flag (uses custom `moosicbox_opus` integration)
- **Other formats**: Additional formats supported by Symphonia's default codecs (WAV, Vorbis, etc.)

## Installation

### Cargo Dependencies

```toml
[dependencies]
moosicbox_audio_decoder = { path = "../audio_decoder" }

# Optional: Enable specific format support
moosicbox_audio_decoder = {
    path = "../audio_decoder",
    features = ["mp3", "flac", "aac", "opus"]
}
```

### Available Feature Flags

```toml
# Format support
aac  = []
flac = []
mp3  = []
opus = ["dep:moosicbox_opus"]

# Convenience features
all-formats    = ["all-os-formats", "mp3"]
all-os-formats = ["aac", "flac", "opus"]

# Development
fail-on-warnings = [
    "moosicbox_opus?/fail-on-warnings",
    "moosicbox_stream_utils/fail-on-warnings",
    "switchy_async/fail-on-warnings",
    "switchy_http/fail-on-warnings",
    "switchy_time/fail-on-warnings",
]
profiling = ["dep:profiling"]
```

## Usage

### Basic File Decoding

```rust
use moosicbox_audio_decoder::{AudioDecodeHandler, decode_file_path_str};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an audio decode handler
    let mut handler = AudioDecodeHandler::new();

    // Add output handler that processes decoded audio
    handler = handler.with_output(Box::new(|spec, duration| {
        // Create your audio output that implements AudioDecode trait
        // spec: SignalSpec (sample rate, channels, etc.)
        // duration: buffer capacity
        Ok(Box::new(MyAudioOutput::new(spec, duration)?))
    }));

    // Decode the file
    decode_file_path_str(
        "path/to/audio.flac",
        &mut handler,
        true,  // enable_gapless
        false, // verify
        None,  // track_num
        None,  // seek position
    )?;

    Ok(())
}
```

### Async File Decoding

```rust
use moosicbox_audio_decoder::{AudioDecodeHandler, decode_file_path_str_async};

async fn decode_async() -> Result<(), Box<dyn std::error::Error>> {
    let result = decode_file_path_str_async(
        "path/to/audio.mp3",
        || {
            let mut handler = AudioDecodeHandler::new();
            handler = handler.with_output(Box::new(|spec, duration| {
                Ok(Box::new(MyAudioOutput::new(spec, duration)?))
            }));
            Ok(handler)
        },
        true,  // enable_gapless
        false, // verify
        None,  // track_num
        None,  // seek position
    ).await?;

    Ok(())
}
```

### Implementing AudioDecode

To process decoded audio, implement the `AudioDecode` trait:

```rust
use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError};
use symphonia::core::audio::AudioBuffer;
use symphonia::core::formats::{Packet, Track};

struct MyAudioOutput {
    // Your state
}

impl AudioDecode for MyAudioOutput {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioDecodeError> {
        // Process the decoded audio buffer
        let samples = decoded.chan(0); // Get channel 0 samples

        // Your audio processing logic here

        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        // Flush any buffered audio
        Ok(())
    }
}
```

### Using Filters

Add audio filters to process decoded audio before output:

```rust
use moosicbox_audio_decoder::AudioDecodeHandler;

let handler = AudioDecodeHandler::new()
    .with_filter(Box::new(|decoded, packet, track| {
        // Modify the audio buffer
        // Example: apply gain, effects, etc.
        Ok(())
    }))
    .with_output(/* ... */);
```

### Async Decoding with Media Source Stream

For more control over the input source, use `decode_media_source_async()`:

```rust
use moosicbox_audio_decoder::{decode_media_source_async, AudioDecodeHandler};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::probe::Hint;
use std::fs::File;

async fn decode_with_media_source() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("audio.flac")?;
    let mss = MediaSourceStream::new(
        Box::new(file),
        MediaSourceStreamOptions::default()
    );

    let mut hint = Hint::new();
    hint.with_extension("flac");

    decode_media_source_async(
        mss,
        &hint,
        || {
            let mut handler = AudioDecodeHandler::new()
                .with_output(/* ... */);
            Ok(handler)
        },
        true,  // enable_gapless
        false, // verify
        None,  // track_num
        None,  // seek position
    ).await?;

    Ok(())
}
```

### Cancellation Support

Use a cancellation token to interrupt decoding:

```rust
use moosicbox_audio_decoder::AudioDecodeHandler;
use switchy_async::util::CancellationToken;

let cancellation_token = CancellationToken::new();
let handler = AudioDecodeHandler::new()
    .with_cancellation_token(cancellation_token.clone())
    .with_output(/* ... */);

// Later, cancel the decoding
cancellation_token.cancel();
```

## Core Types

### AudioDecodeHandler

The main handler for managing audio decoding pipeline:

```rust
pub struct AudioDecodeHandler {
    pub cancellation_token: Option<CancellationToken>,
    // ... private fields
}

impl AudioDecodeHandler {
    pub fn new() -> Self;
    pub fn with_filter(self, filter: AudioFilter) -> Self;
    pub fn with_output(self, open_output: OpenAudioDecodeHandler) -> Self;
    pub fn with_cancellation_token(self, cancellation_token: CancellationToken) -> Self;
}
```

### AudioDecode Trait

Trait for implementing audio output handlers:

```rust
pub trait AudioDecode {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioDecodeError>;

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        Ok(())
    }
}
```

### Errors

```rust
pub enum AudioDecodeError {
    OpenStream,
    PlayStream,
    StreamClosed,
    StreamEnd,
    Interrupt,
    IO(std::io::Error),
    Other(Box<dyn std::error::Error + Send + Sync>),
}

pub enum DecodeError {
    AudioDecode(AudioDecodeError),
    Symphonia(symphonia::core::errors::Error),
    Join(JoinError),
    NoAudioOutputs,
    InvalidSource,
}
```

## Media Sources

The package includes support for various media sources:

- **`bytestream_source`**: Bytestream-based media sources
- **`remote_bytestream`**: Remote bytestream support for network sources
- **`streamable_file_async`**: Async file streaming support

These can be used with Symphonia's `MediaSourceStream` for flexible input handling.

## Dependencies

Core dependencies:

- **symphonia**: Media demuxing and decoding framework
- **moosicbox_stream_utils**: Streaming utilities (remote bytestream, stalled monitoring)
- **tokio**: Async runtime
- **bytes**: Byte buffer utilities
- **flume**: MPSC channels
- **futures**: Async utilities

## Limitations

This package is a **wrapper around Symphonia** and does not implement custom decoders. It provides:

- Handler-based architecture for audio output management
- Integration with MoosicBox streaming infrastructure
- Async support for blocking operations
- Cancellation support

For advanced features like custom resampling, format conversion, or metadata extraction, you'll need to implement these in your `AudioDecode` implementation or use additional libraries.

## See Also

- [Symphonia](https://github.com/pdeljanov/Symphonia) - The underlying media framework
- [MoosicBox Stream Utils](../stream_utils/README.md) - Streaming utilities
- [MoosicBox Player](../player/README.md) - Audio playback engine
