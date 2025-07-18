# MoosicBox Audio Output

A cross-platform audio output abstraction layer supporting multiple audio backends for high-quality audio playback.

## Overview

The MoosicBox Audio Output package provides:

- **Cross-Platform Support**: Works on Windows, macOS, and Linux
- **Multiple Audio Backends**: CPAL, PulseAudio, JACK, ASIO support
- **Low-Latency Playback**: Optimized for real-time audio applications
- **Format Flexibility**: Support for various sample rates, bit depths, and channel configurations
- **Device Management**: Enumerate and select audio output devices
- **Real-Time Processing**: Minimal latency audio pipeline

## Supported Audio Backends

### CPAL (Cross-Platform Audio Library)
- **Platforms**: Windows, macOS, Linux
- **Use Case**: General-purpose audio output with good cross-platform compatibility
- **Latency**: Medium (suitable for most applications)

### PulseAudio
- **Platforms**: Linux (primary), some Unix-like systems
- **Use Case**: Desktop Linux audio with system integration
- **Features**: Volume control, device switching, network audio

### PulseAudio Simple
- **Platforms**: Linux
- **Use Case**: Simplified PulseAudio interface for basic playback
- **Benefits**: Lower overhead, easier integration

### JACK (JACK Audio Connection Kit)
- **Platforms**: Linux, macOS, Windows
- **Use Case**: Professional audio, low-latency applications
- **Features**: Real-time audio routing, minimal latency

### ASIO (Audio Stream Input/Output)
- **Platforms**: Windows
- **Use Case**: Professional audio interfaces, ultra-low latency
- **Requirements**: ASIO-compatible audio hardware

## Usage

### Basic Audio Output

```rust
use moosicbox_audio_output::{AudioOutput, AudioOutputConfig, AudioBackend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create audio output configuration
    let config = AudioOutputConfig {
        backend: AudioBackend::Cpal,
        sample_rate: 44100,
        channels: 2,
        bit_depth: 16,
        buffer_size: 1024,
        device_name: None, // Use default device
    };

    // Initialize audio output
    let mut audio_output = AudioOutput::new(config).await?;

    // Play audio samples
    let samples = vec![0.0f32; 1024]; // Silent audio
    audio_output.write_samples(&samples).await?;

    Ok(())
}
```

### Device Enumeration

```rust
use moosicbox_audio_output::{AudioOutput, list_audio_devices};

async fn list_available_devices() -> Result<(), Box<dyn std::error::Error>> {
    // List all available audio devices
    let devices = list_audio_devices(AudioBackend::Cpal).await?;

    for device in devices {
        println!("Device: {} ({})", device.name, device.id);
        println!("  Channels: {}", device.max_channels);
        println!("  Sample rates: {:?}", device.supported_sample_rates);
        println!("  Default: {}", device.is_default);
    }

    Ok(())
}
```

### Advanced Configuration

```rust
use moosicbox_audio_output::{AudioOutputConfig, AudioBackend, AudioLatency};

fn create_low_latency_config() -> AudioOutputConfig {
    AudioOutputConfig {
        backend: AudioBackend::Jack, // Use JACK for low latency
        sample_rate: 48000,
        channels: 2,
        bit_depth: 24,
        buffer_size: 64, // Small buffer for low latency
        device_name: Some("JACK Audio".to_string()),
        latency: AudioLatency::Minimal,
        exclusive_mode: true, // Request exclusive access
    }
}

fn create_high_quality_config() -> AudioOutputConfig {
    AudioOutputConfig {
        backend: AudioBackend::Asio, // Use ASIO for best quality
        sample_rate: 192000, // High sample rate
        channels: 8, // Surround sound
        bit_depth: 32,
        buffer_size: 2048, // Larger buffer for stability
        device_name: Some("RME Audio Interface".to_string()),
        latency: AudioLatency::Quality,
        exclusive_mode: true,
    }
}
```

### Real-Time Audio Processing

```rust
use moosicbox_audio_output::{AudioOutput, AudioProcessor};

struct CustomAudioProcessor {
    // Your audio processing state
}

impl AudioProcessor for CustomAudioProcessor {
    fn process_samples(&mut self, input: &[f32], output: &mut [f32]) {
        // Apply audio effects, volume control, etc.
        for (i, sample) in input.iter().enumerate() {
            output[i] = sample * 0.8; // Simple volume reduction
        }
    }
}

async fn setup_real_time_processing() -> Result<(), Box<dyn std::error::Error>> {
    let config = AudioOutputConfig::default();
    let mut audio_output = AudioOutput::new(config).await?;

    let processor = CustomAudioProcessor {};
    audio_output.set_processor(Box::new(processor))?;

    // Audio will now be processed in real-time
    Ok(())
}
```

## Feature Flags

### Audio Backends
- `cpal` - Enable CPAL backend (cross-platform)
- `jack` - Enable JACK backend (professional audio)
- `asio` - Enable ASIO backend (Windows professional)

### Audio Formats
- `aac` - Support for AAC audio format
- `flac` - Support for FLAC audio format
- `mp3` - Support for MP3 audio format
- `opus` - Support for Opus audio format

### Convenience Features
- `all-backends` - Enable all available audio backends
- `default-backend` - Enable the recommended backend for the platform

## Configuration

### Audio Quality Settings

```rust
// CD Quality
let cd_quality = AudioOutputConfig {
    sample_rate: 44100,
    bit_depth: 16,
    channels: 2,
    ..Default::default()
};

// Hi-Res Audio
let hires_quality = AudioOutputConfig {
    sample_rate: 96000,
    bit_depth: 24,
    channels: 2,
    ..Default::default()
};

// Studio Quality
let studio_quality = AudioOutputConfig {
    sample_rate: 192000,
    bit_depth: 32,
    channels: 2,
    ..Default::default()
};
```

### Latency Optimization

```rust
// Low latency for real-time applications
let low_latency = AudioOutputConfig {
    buffer_size: 64,   // Very small buffer
    latency: AudioLatency::Minimal,
    backend: AudioBackend::Jack,
    ..Default::default()
};

// Balanced latency and stability
let balanced = AudioOutputConfig {
    buffer_size: 512,  // Medium buffer
    latency: AudioLatency::Balanced,
    backend: AudioBackend::Cpal,
    ..Default::default()
};

// High stability for background playback
let stable = AudioOutputConfig {
    buffer_size: 4096, // Large buffer
    latency: AudioLatency::Quality,
    backend: AudioBackend::PulseAudio,
    ..Default::default()
};
```

## Platform-Specific Notes

### Linux
- **PulseAudio**: Most common on desktop Linux distributions
- **JACK**: Preferred for professional audio work
- **ALSA**: Lower-level access (via CPAL)

```bash
# Install PulseAudio development headers
sudo apt-get install libpulse-dev

# Install JACK development headers
sudo apt-get install libjack-jackd2-dev
```

### macOS
- **Core Audio**: Native macOS audio (via CPAL)
- **JACK**: Available but requires separate installation

```bash
# Install JACK (optional)
brew install jack
```

### Windows
- **WASAPI**: Modern Windows audio API (via CPAL)
- **DirectSound**: Legacy Windows audio (via CPAL)
- **ASIO**: Professional audio interfaces

For ASIO support, you need:
- ASIO-compatible audio hardware
- ASIO drivers from hardware manufacturer

## Error Handling

```rust
use moosicbox_audio_output::error::AudioOutputError;

match audio_output.write_samples(&samples).await {
    Ok(()) => println!("Audio written successfully"),
    Err(AudioOutputError::DeviceNotFound(device)) => {
        eprintln!("Audio device not found: {}", device);
    },
    Err(AudioOutputError::UnsupportedFormat { sample_rate, channels, bit_depth }) => {
        eprintln!("Unsupported format: {}Hz, {} channels, {} bits",
                  sample_rate, channels, bit_depth);
    },
    Err(AudioOutputError::BufferUnderrun) => {
        eprintln!("Audio buffer underrun - increase buffer size");
    },
    Err(AudioOutputError::DeviceBusy) => {
        eprintln!("Audio device is busy - try exclusive_mode: false");
    },
    Err(e) => {
        eprintln!("Audio output error: {}", e);
    }
}
```

## Performance Optimization

### Buffer Size Guidelines
- **64-128 samples**: Ultra-low latency (professional use)
- **256-512 samples**: Low latency (real-time applications)
- **1024-2048 samples**: Balanced (most applications)
- **4096+ samples**: High stability (background playback)

### Sample Rate Considerations
- **44100 Hz**: CD quality, widely supported
- **48000 Hz**: Professional standard, good compatibility
- **96000 Hz**: Hi-res audio, higher CPU usage
- **192000 Hz**: Studio quality, significant CPU usage

### Memory Usage
- Buffer size directly affects memory usage
- Higher sample rates increase memory requirements
- Multiple channels multiply memory usage

## Integration with MoosicBox

### Player Integration

```rust
use moosicbox_audio_output::AudioOutput;
use moosicbox_player::Player;

async fn setup_player_with_audio_output() -> Result<(), Box<dyn std::error::Error>> {
    // Create audio output
    let audio_config = AudioOutputConfig {
        backend: AudioBackend::Cpal,
        sample_rate: 44100,
        channels: 2,
        bit_depth: 16,
        buffer_size: 1024,
        ..Default::default()
    };

    let audio_output = AudioOutput::new(audio_config).await?;

    // Create player with audio output
    let player = Player::with_audio_output(audio_output).await?;

    Ok(())
}
```

## Troubleshooting

### Common Issues

1. **No audio output**: Check device selection and permissions
2. **Audio crackling**: Increase buffer size or check sample rate compatibility
3. **High latency**: Use JACK or ASIO backends, reduce buffer size
4. **Device not found**: Verify device name and availability

### Debug Information

```bash
# Enable audio output debugging
RUST_LOG=moosicbox_audio_output=debug cargo run

# List available audio devices
RUST_LOG=moosicbox_audio_output=debug cargo run -- --list-devices

# Test audio output
RUST_LOG=moosicbox_audio_output=debug cargo run -- --test-audio
```

### Platform-Specific Troubleshooting

#### Linux
```bash
# Check PulseAudio status
pulseaudio --check -v

# List PulseAudio devices
pactl list sinks

# Test JACK connection
jack_control status
```

#### Windows
```bash
# Check Windows audio devices
powershell "Get-WmiObject Win32_SoundDevice"
```

#### macOS
```bash
# List Core Audio devices
system_profiler SPAudioDataType
```

## See Also

- [MoosicBox Player](../player/README.md) - Audio playback engine that uses this library
- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio format decoding
- [MoosicBox Server](../server/README.md) - Server that integrates audio output
