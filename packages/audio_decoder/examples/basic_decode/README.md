# Basic Audio Decoding Example

This example demonstrates the core functionality of the `moosicbox_audio_decoder` package by showing how to decode an audio file and process the decoded samples.

## Summary

This example implements a simple audio decoder that collects statistics about decoded audio (packet count, sample count, duration) and demonstrates the fundamental workflow of using the audio decoder API.

## What This Example Demonstrates

- Implementing the `AudioDecode` trait to process decoded audio buffers
- Creating and configuring an `AudioDecodeHandler`
- Using `decode_file_path_str()` to decode audio from a file path
- Accessing decoded audio samples from `AudioBuffer<f32>`
- Understanding the decoder lifecycle (initialization, decoding, flushing)
- Extracting audio metadata (sample rate, channels, frame count)
- Processing audio packets in real-time as they are decoded

## Prerequisites

- Rust toolchain (2021 edition or later)
- An audio file to decode (MP3, FLAC, AAC, Opus, WAV, etc.)
- Basic understanding of audio concepts (sample rate, channels, frames)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/audio_decoder/examples/basic_decode/Cargo.toml -- path/to/your/audio.flac
```

Or from the example directory:

```bash
cd packages/audio_decoder/examples/basic_decode
cargo run -- path/to/your/audio.flac
```

### Example with different audio formats

```bash
# Decode FLAC file
cargo run -- song.flac

# Decode MP3 file
cargo run -- song.mp3

# Decode Opus file
cargo run -- song.opus
```

## Expected Output

When you run the example, you should see output similar to:

```
MoosicBox Audio Decoder - Basic Example
========================================

Decoding file: song.flac

Initializing decoder:
  Sample rate: 44100 Hz
  Channels: 2

Decoded 100 packets (220200 samples)...
Decoded 200 packets (441000 samples)...
Decoded 300 packets (661800 samples)...
...
Flushing decoder...

Decoding result code: 0
```

The example prints progress every 100 packets and shows the total number of samples decoded.

## Code Walkthrough

### 1. Implementing the `AudioDecode` Trait

The core of this example is the `SimpleAudioDecoder` struct that implements `AudioDecode`:

```rust
struct SimpleAudioDecoder {
    packet_count: usize,
    sample_count: usize,
    sample_rate: u32,
    channels: usize,
}

impl AudioDecode for SimpleAudioDecoder {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        // Process each decoded audio packet
        let frames = decoded.frames();
        let channels = decoded.spec().channels.count();

        self.packet_count += 1;
        self.sample_count += frames * channels;

        // Access samples from a specific channel
        if let Some(channel_samples) = decoded.chan(0) {
            // Process the samples...
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        // Clean up and flush buffered audio
        Ok(())
    }
}
```

**Key Points:**

- `decoded()` is called for each audio packet successfully decoded
- `AudioBuffer<f32>` contains the decoded samples as 32-bit floats
- `frames()` returns the number of samples per channel
- `chan(n)` accesses samples for channel `n`
- `flush()` is called at the end to ensure all audio is processed

### 2. Creating the Audio Decode Handler

The handler manages the decoding pipeline:

```rust
let mut handler = AudioDecodeHandler::new()
    .with_output(Box::new(|spec, _duration| {
        // Called when audio format is determined
        let decoder = SimpleAudioDecoder::new(
            spec.rate,           // Sample rate (e.g., 44100 Hz)
            spec.channels.count(), // Number of channels (e.g., 2 for stereo)
        );
        Ok(Box::new(decoder))
    }));
```

The closure passed to `with_output()` is called once Symphonia has probed the audio file and determined its format. This is when you know the actual sample rate and channel configuration.

### 3. Decoding the File

The `decode_file_path_str()` function performs the actual decoding:

```rust
let result = decode_file_path_str(
    file_path,
    &mut handler,
    true,  // enable_gapless: Accurate timing, no gaps between packets
    false, // verify: Enable/disable decoder verification (slower but safer)
    None,  // track_num: Select specific track (None = first audio track)
    None,  // seek: Seek position in seconds (None = start from beginning)
)?;
```

**Parameters explained:**

- `file_path`: Path to the audio file to decode
- `handler`: The `AudioDecodeHandler` with your custom decoder
- `enable_gapless`: When true, enables accurate gapless playback
- `verify`: When true, enables decoder verification for debugging
- `track_num`: Optional track number to select (for multi-track files)
- `seek`: Optional seek position in seconds

### 4. Understanding the Decode Flow

1. **Initialization**: `decode_file_path_str()` opens the file and probes the format
2. **Handler Creation**: The closure in `with_output()` is called with audio specs
3. **Decoding Loop**: For each packet in the file:
    - Symphonia decodes the packet
    - Your `decoded()` method is called with the audio buffer
    - You process the samples
4. **Finalization**: Your `flush()` method is called to clean up

## Key Concepts

### AudioBuffer and Samples

The `AudioBuffer<f32>` contains decoded audio as 32-bit floating-point samples normalized to the range [-1.0, 1.0]:

- `decoded.frames()`: Number of samples per channel in this buffer
- `decoded.spec()`: Audio specification (sample rate, channels, format)
- `decoded.chan(n)`: Get samples for channel `n` as a slice `&[f32]`

### Packets vs Frames vs Samples

- **Packet**: Compressed audio data read from the file
- **Frame**: One sample for each channel (stereo has 2 samples per frame)
- **Sample**: A single audio value for one channel

For stereo audio at 44.1kHz:

- 1 second = 44,100 frames
- 1 second = 88,200 samples (44,100 frames × 2 channels)

### Gapless Playback

When `enable_gapless` is true, the decoder removes encoder delay and padding, providing accurate timing without gaps between tracks. This is important for albums or podcasts.

### Decoder Verification

When `verify` is true, the decoder performs additional checks to ensure the audio is decoded correctly. This is slower but useful for debugging or validating audio files.

## Testing the Example

1. **Test with different formats**: Try MP3, FLAC, AAC, Opus, WAV files
2. **Test with different configurations**:
    - Mono vs stereo files
    - Different sample rates (44.1kHz, 48kHz, 96kHz)
    - Different bit depths
3. **Enable logging**: Set `RUST_LOG=debug` to see detailed decoder logs:
    ```bash
    RUST_LOG=debug cargo run -- song.flac
    ```
4. **Try decoder verification**:
    - Modify the code to set `verify: true` in `decode_file_path_str()`
    - This will show any decoder warnings or errors

## Troubleshooting

### "No such file or directory"

Make sure the audio file path is correct and the file exists.

### "the input is not supported"

The audio format may not be supported by the enabled features. Check that the necessary format features are enabled in `Cargo.toml` (the default Symphonia codecs support many common formats).

### Compilation errors with Symphonia types

Make sure you're using the workspace version of Symphonia that matches the version used by `moosicbox_audio_decoder`.

### Silent output or no audio playing

This example only collects statistics - it doesn't play audio. To play audio, you would need to:

- Use `moosicbox_audio_output` package for hardware playback
- Write samples to a WAV file
- Use an encoder to convert to another format

## Related Examples

This is currently the only example for `moosicbox_audio_decoder`. Future examples may include:

- Async decoding with `decode_file_path_str_async()`
- Using media sources for streaming audio
- Applying audio filters during decoding
- Using cancellation tokens to stop decoding
- Seeking within audio files
- Decoding from remote/HTTP sources

## Next Steps

To build on this example:

1. **Add audio output**: Integrate with `moosicbox_audio_output` to play audio through your speakers
2. **Write to file**: Save decoded samples to a WAV file or use an encoder
3. **Apply filters**: Use `handler.with_filter()` to modify audio (gain, effects)
4. **Add seeking**: Implement UI controls to seek within the audio file
5. **Use async API**: Convert to use `decode_file_path_str_async()` for better responsiveness
6. **Stream from network**: Use media sources to decode audio from HTTP/remote sources
