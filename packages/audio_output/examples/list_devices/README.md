# Audio Device Enumeration Example

## Summary

This example demonstrates how to scan for and enumerate available audio output devices using the `moosicbox_audio_output` package. It shows device specifications including sample rate, channel count, and channel layout.

## What This Example Demonstrates

- Scanning for available audio output devices using `scan_outputs()`
- Retrieving all available audio output factories with `output_factories()`
- Getting the default audio output device with `default_output_factory()`
- Accessing device properties (name, ID, sample rate, channels)
- Displaying channel layout information

## Prerequisites

- Basic understanding of async/await in Rust
- Tokio runtime knowledge
- An audio output device (speakers, headphones, etc.) connected to your system

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/audio_output/examples/list_devices/Cargo.toml
```

For verbose logging to see internal operations:

```bash
RUST_LOG=moosicbox_audio_output=debug cargo run --manifest-path packages/audio_output/examples/list_devices/Cargo.toml
```

## Expected Output

When you run this example, you should see output similar to:

```
MoosicBox Audio Output - Device Enumeration Example
====================================================

Scanning for audio output devices...
Scan complete!

Found 2 audio output device(s):

Device 1: Speakers (Realtek High Definition Audio)
  ID: {0.0.0.00000000}.{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}
  Sample Rate: 48000 Hz
  Channels: 2
  Channel Layout: Stereo

Device 2: Headphones (USB Audio Device)
  ID: {0.0.1.00000000}.{yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy}
  Sample Rate: 44100 Hz
  Channels: 2
  Channel Layout: Stereo

Default Audio Output Device:
  Name: Speakers (Realtek High Definition Audio)
  ID: {0.0.0.00000000}.{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}
  Sample Rate: 48000 Hz
  Channels: 2
  Channel Layout: Stereo

Example completed successfully!
```

The actual output will vary based on your system's audio configuration and connected devices.

## Code Walkthrough

### Initialization

```rust
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
```

This initializes the logger to display information about the audio device scanning process.

### Scanning for Devices

```rust
scan_outputs().await?;
```

The `scan_outputs()` function scans the system for available audio output devices. This populates an internal registry that can be queried for available devices. The scan happens asynchronously and may take a moment on some systems.

### Retrieving Device List

```rust
let factories = output_factories().await;
```

After scanning, `output_factories()` returns a list of `AudioOutputFactory` instances. Each factory can be used to create an `AudioOutput` instance for that device.

### Accessing Device Properties

```rust
for (index, factory) in factories.iter().enumerate() {
    println!("Device {}: {}", index + 1, factory.name);
    println!("  ID: {}", factory.id);
    println!("  Sample Rate: {} Hz", factory.spec.rate);
    println!("  Channels: {}", factory.spec.channels.count());
}
```

Each `AudioOutputFactory` provides:

- `name`: Human-readable device name
- `id`: Unique device identifier
- `spec`: Audio signal specification including:
    - `rate`: Sample rate in Hz (e.g., 44100, 48000)
    - `channels`: Channel configuration (stereo, mono, surround, etc.)

### Getting the Default Device

```rust
if let Some(default_factory) = default_output_factory().await {
    println!("Default Audio Output Device:");
    println!("  Name: {}", default_factory.name);
}
```

The `default_output_factory()` function returns the system's default audio output device, which is typically what you want to use for playback unless the user selects a specific device.

## Key Concepts

### Audio Output Factory

An `AudioOutputFactory` is a factory pattern implementation that defers the creation of the actual audio output until needed. This is useful because:

- Audio devices are system resources that should be acquired only when needed
- The factory can be cloned and passed around without holding device resources
- Multiple outputs can be created from the same factory

### Signal Specification

The `SignalSpec` structure from Symphonia defines the audio format:

- **Sample Rate**: Number of audio samples per second (Hz). Common values are 44100 Hz (CD quality) and 48000 Hz (professional audio)
- **Channels**: Audio channel configuration (mono, stereo, 5.1 surround, etc.)

### Device Scanning

Device scanning is an asynchronous operation because:

- It may need to query hardware interfaces
- Some backends (like JACK) may need to connect to services
- Multiple backends may be scanned in parallel

## Testing the Example

1. Run the example with different audio devices connected
2. Disconnect/reconnect devices and run again to see the list change
3. Try the debug logging to see internal CPAL operations:
    ```bash
    RUST_LOG=moosicbox_audio_output=debug cargo run --manifest-path packages/audio_output/examples/list_devices/Cargo.toml
    ```

## Troubleshooting

### No devices found

If no devices are detected:

- Verify that audio devices are connected and enabled in your system settings
- On Linux, check that ALSA is properly configured: `aplay -l`
- On macOS, check System Preferences > Sound
- On Windows, check Sound settings in Control Panel

### Permission errors

On some systems, accessing audio devices may require specific permissions:

- **Linux**: Ensure your user is in the `audio` group: `sudo usermod -a -G audio $USER`
- **macOS**: Grant microphone permissions in System Preferences > Security & Privacy (even for output-only access)

### Build errors

If you encounter build errors:

- Ensure you have the required audio system libraries installed
- On Linux: `sudo apt-get install libasound2-dev`
- See the main package README for platform-specific dependencies

## Related Examples

This is currently the only example for `moosicbox_audio_output`. Future examples may include:

- Audio playback with real audio data
- Volume control and command handling
- Progress tracking during playback
- Custom audio encoders
