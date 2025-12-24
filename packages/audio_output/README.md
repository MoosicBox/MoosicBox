# MoosicBox Audio Output

A cross-platform audio output abstraction layer for high-quality audio playback.

## Overview

The MoosicBox Audio Output package provides:

- **Cross-Platform Support**: Works on Windows, macOS, and Linux
- **CPAL-Based Audio Output**: Uses CPAL (Cross-Platform Audio Library) for audio playback
- **Professional Audio Backend Support**: Optional JACK and ASIO support via CPAL
- **Audio Format Encoding**: Built-in support for encoding to AAC, FLAC, MP3, and Opus
- **Device Management**: Scan and select audio output devices
- **Command-Based Control**: Pause, resume, seek, and volume control via `AudioHandle`
- **Progress Tracking**: Real-time playback position tracking with callbacks

## Supported Audio Backends

All audio backends are provided through CPAL (Cross-Platform Audio Library):

### CPAL (Cross-Platform Audio Library)

- **Platforms**: Windows (WASAPI, DirectSound), macOS (Core Audio), Linux (ALSA)
- **Use Case**: General-purpose audio output with good cross-platform compatibility
- **Default Backend**: Enabled by default

### JACK (JACK Audio Connection Kit)

- **Platforms**: Linux, macOS, Windows
- **Use Case**: Professional audio, low-latency applications
- **Features**: Real-time audio routing, minimal latency
- **Enabled via**: `jack` feature flag

### ASIO (Audio Stream Input/Output)

- **Platforms**: Windows
- **Use Case**: Professional audio interfaces, ultra-low latency
- **Requirements**: ASIO-compatible audio hardware
- **Enabled via**: `asio` feature flag

## Usage

### Basic Audio Output

```rust
use moosicbox_audio_output::{scan_outputs, default_output};
use moosicbox_audio_output::AudioWrite;
use symphonia::core::audio::AudioBuffer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Scan available audio outputs
    scan_outputs().await?;

    // Get the default audio output
    let mut audio_output = default_output().await?;

    // Write audio samples (expects AudioBuffer<f32> from Symphonia)
    // The AudioBuffer would typically come from an audio decoder
    // audio_output.write(audio_buffer)?;

    // Flush when done
    audio_output.flush()?;

    Ok(())
}
```

### Device Enumeration

```rust
use moosicbox_audio_output::{scan_outputs, output_factories};

async fn list_available_devices() -> Result<(), Box<dyn std::error::Error>> {
    // Scan available audio outputs
    scan_outputs().await?;

    // List all available audio output factories
    let factories = output_factories().await;

    for factory in factories {
        println!("Device: {} ({})", factory.name, factory.id);
        println!("  Sample rate: {} Hz", factory.spec.rate);
        println!("  Channels: {}", factory.spec.channels.count());
    }

    Ok(())
}
```

### Volume and Playback Control

```rust
use moosicbox_audio_output::{default_output, AudioWrite};

async fn playback_control_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut audio_output = default_output().await?;

    // Get the audio handle for command-based control
    let handle = audio_output.handle();

    // Set volume (0.0 to 1.0)
    handle.set_volume(0.8).await?;

    // Pause playback
    handle.pause().await?;

    // Resume playback
    handle.resume().await?;

    // Seek to position (in seconds)
    handle.seek(30.0).await?;

    // Reset the audio output
    handle.reset().await?;

    Ok(())
}
```

### Progress Tracking

```rust
use moosicbox_audio_output::{default_output, AudioWrite};

async fn progress_tracking_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut audio_output = default_output().await?;

    // Set a progress callback that gets called with playback position (in seconds)
    audio_output.set_progress_callback(Some(Box::new(|position| {
        println!("Current playback position: {:.2}s", position);
    })));

    // Get current playback position
    if let Some(position) = audio_output.get_playback_position() {
        println!("Position: {:.2}s", position);
    }

    Ok(())
}
```

## Feature Flags

### Audio Backends

- `cpal` - Enable CPAL backend (cross-platform, enabled by default)
- `jack` - Enable JACK backend via CPAL (professional audio)
- `asio` - Enable ASIO backend via CPAL (Windows professional audio)

### Audio Encoding Formats

- `aac` - Support for encoding to AAC format
- `flac` - Support for encoding to FLAC format
- `mp3` - Support for encoding to MP3 format
- `opus` - Support for encoding to Opus format

### API Features

- `api` - Enable API models for integration (enabled by default)
- `openapi` - Enable OpenAPI/utoipa support (enabled by default)

### Default Features

The `default` feature enables: `api`, `default-windows`, and `openapi`.

The `default-windows` feature enables: `aac`, `cpal`, `flac`, `mp3`, and `opus`.

## Architecture

The audio output package uses a layered architecture:

1. **AudioOutput**: Main wrapper that handles resampling and delegates to `AudioWrite`
2. **AudioWrite**: Core trait for writing audio buffers to output devices
3. **AudioOutputFactory**: Factory pattern for creating audio outputs with specific configurations
4. **CpalAudioOutput**: CPAL-based implementation of `AudioWrite`
5. **AudioHandle**: Command-based interface for controlling playback (pause, resume, volume, seek)
6. **ProgressTracker**: Tracks and reports playback progress with callbacks

### Audio Specifications

The package uses Symphonia's `SignalSpec` for audio specifications:

```rust
use symphonia::core::audio::SignalSpec;

// The SignalSpec defines the audio format
let spec = SignalSpec {
    rate: 44100,  // Sample rate in Hz
    channels: symphonia::core::audio::Layout::Stereo.into_channels(),
};
```

Audio quality is determined by the source material and the audio device's capabilities. The package automatically handles:

- Sample rate conversion (resampling)
- Channel configuration
- Buffer management (30-second ring buffer with 10-second initial buffering)

## Platform-Specific Notes

### Linux

- **ALSA**: Default backend via CPAL (direct hardware access)
- **JACK**: Optional professional audio backend (enable with `jack` feature)

```bash
# JACK is optional - install only if you need professional audio features
sudo apt-get install libjack-jackd2-dev
```

### macOS

- **Core Audio**: Native macOS audio (via CPAL)
- **JACK**: Optional professional audio backend (enable with `jack` feature)

```bash
# JACK is optional - install only if you need professional audio features
brew install jack
```

### Windows

- **WASAPI**: Modern Windows audio API (default via CPAL)
- **DirectSound**: Legacy Windows audio (fallback via CPAL)
- **ASIO**: Optional professional audio backend (enable with `asio` feature)

For ASIO support:

- ASIO-compatible audio hardware
- ASIO drivers from hardware manufacturer
- Enable the `asio` feature flag in Cargo.toml

## Error Handling

```rust
use moosicbox_audio_output::AudioOutputError;

match audio_output.write(audio_buffer) {
    Ok(samples_written) => println!("Audio written successfully: {} samples", samples_written),
    Err(AudioOutputError::NoOutputs) => {
        eprintln!("No audio outputs available");
    },
    Err(AudioOutputError::UnsupportedOutputConfiguration) => {
        eprintln!("Unsupported output configuration");
    },
    Err(AudioOutputError::UnsupportedChannels(channels)) => {
        eprintln!("Unsupported channel count: {}", channels);
    },
    Err(AudioOutputError::OpenStream) => {
        eprintln!("Failed to open audio stream");
    },
    Err(AudioOutputError::PlayStream) => {
        eprintln!("Failed to play audio stream");
    },
    Err(AudioOutputError::StreamClosed) => {
        eprintln!("Audio stream was closed");
    },
    Err(AudioOutputError::StreamEnd) => {
        eprintln!("Audio stream ended");
    },
    Err(e) => {
        eprintln!("Audio output error: {}", e);
    }
}
```

## Implementation Details

### Buffer Management

The CPAL implementation uses a ring buffer architecture:

- **Ring buffer size**: 30 seconds of audio
- **Initial buffering**: 10 seconds before playback starts
- **Purpose**: Prevents audio underruns and ensures smooth playback

This approach ensures that:

- Short audio clips (< 10 seconds) start immediately on flush
- Long audio content has ample buffering to prevent crackling
- Volume changes are applied immediately in the CPAL callback

### Sample Rate Handling

The package automatically handles sample rate conversion:

- Uses the `moosicbox_resampler` crate for high-quality resampling
- Converts input audio to match the output device's sample rate
- Maintains audio quality during conversion

### Progress Tracking

Progress tracking uses a dedicated `ProgressTracker`:

- Tracks consumed samples from the CPAL audio callback
- Calculates playback position based on actual output sample rate
- Triggers callbacks when position changes by ≥0.1 seconds
- Thread-safe via atomic operations

## Integration with MoosicBox

The audio output package integrates with other MoosicBox components:

- **moosicbox_audio_decoder**: Provides decoded audio buffers via the `AudioDecode` trait
- **moosicbox_resampler**: Handles sample rate conversion automatically
- **moosicbox_player**: Uses `AudioOutput` for playback
- **moosicbox_stream_utils**: Provides streaming utilities

The `AudioOutput` implements the `AudioDecode` trait, allowing it to receive decoded audio directly from audio decoders.

## Troubleshooting

### Common Issues

1. **No audio output**:
    - Run `scan_outputs().await` to ensure devices are detected
    - Check system audio settings and permissions
    - Verify CPAL feature is enabled

2. **Audio crackling or stuttering**:
    - This is typically a system resource issue
    - The package uses a 30-second ring buffer with 10-second initial buffering to prevent this
    - Check CPU usage and system load

3. **Device not found**:
    - Verify the device is connected and enabled in system settings
    - Run `output_factories().await` to list available devices

4. **Build errors with JACK/ASIO**:
    - Ensure you have the required development libraries installed
    - JACK requires `libjack-jackd2-dev` on Linux
    - ASIO requires ASIO SDK and drivers on Windows

### Debug Logging

Enable debug logging to troubleshoot issues:

```bash
# Enable audio output debugging
RUST_LOG=moosicbox_audio_output=debug cargo run

# Enable trace-level logging for detailed information
RUST_LOG=moosicbox_audio_output=trace cargo run
```

### Platform-Specific Troubleshooting

#### Linux

```bash
# List ALSA devices
aplay -l

# Check JACK status (if using JACK feature)
jack_control status
```

#### Windows

```bash
# List audio devices
powershell "Get-WmiObject Win32_SoundDevice"
```

#### macOS

```bash
# List audio devices
system_profiler SPAudioDataType
```

## See Also

- [MoosicBox Player](../player/README.md) - Audio playback engine that uses this library
- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio format decoding
- [MoosicBox Server](../server/README.md) - Server that integrates audio output
