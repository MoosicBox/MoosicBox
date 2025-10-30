# Basic Audio Decoding Example

This example demonstrates the fundamental usage of the `moosicbox_audio_decoder` package to decode audio files and process decoded samples.

## Summary

This example shows how to decode an audio file using the `AudioDecodeHandler` and implement a simple `AudioDecode` trait to process the decoded audio data. It counts and reports the total number of samples and frames decoded.

## What This Example Demonstrates

- Creating and configuring an `AudioDecodeHandler`
- Implementing the `AudioDecode` trait to process decoded audio
- Using `decode_file_path_str()` to decode audio files
- Accessing audio format information (sample rate, channels, layout)
- Processing decoded audio buffers frame-by-frame
- Implementing the `flush()` method for cleanup and final reporting
- Handling various audio formats through Symphonia

## Prerequisites

- Basic understanding of audio concepts (samples, frames, channels)
- Familiarity with Rust traits and error handling
- An audio file to test with (FLAC, MP3, WAV, AAC, Opus, etc.)

## Running the Example

```bash
cargo run --manifest-path packages/audio_decoder/examples/basic_usage/Cargo.toml -- /path/to/audio.flac
```

Replace `/path/to/audio.flac` with the path to any audio file you want to decode.

## Expected Output

```
Decoding audio file: /path/to/audio.flac

Audio format detected:
  Sample rate: 44100 Hz
  Channels: 2
  Channel layout: Channels(FRONT_LEFT | FRONT_RIGHT)

Decoding (. = 1 second of audio):
.........

Decoding complete!
  Total frames: 397890
  Total samples: 795780
  Duration: 9.02 seconds
```

The output shows:

- The detected audio format specifications
- Progress dots (one per second of audio)
- Final statistics about the decoded audio

## Code Walkthrough

### 1. Implementing the AudioDecode Trait

The `SampleCounter` struct implements the `AudioDecode` trait to process decoded audio:

```rust
struct SampleCounter {
    sample_count: usize,
    frame_count: usize,
    channels: usize,
    sample_rate: u32,
}

impl AudioDecode for SampleCounter {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        // Count frames and samples
        let frames = decoded.frames();
        self.frame_count += frames;
        self.sample_count += frames * self.channels;

        // Print progress
        // ...

        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        // Print final statistics
        println!("Decoding complete!");
        println!("  Total frames: {}", self.frame_count);
        // ...
        Ok(())
    }
}
```

The `decoded()` method is called for each decoded packet, and `flush()` is called when decoding completes.

### 2. Creating the AudioDecodeHandler

The handler manages the decoding pipeline:

```rust
let mut handler = AudioDecodeHandler::new();

handler = handler.with_output(Box::new(|spec, _duration| {
    // spec contains: sample_rate, channels, channel_layout
    let channels = spec.channels.count();
    let sample_rate = spec.rate;

    // Create and return our decoder implementation
    Ok(Box::new(SampleCounter::new(channels, sample_rate)))
}));
```

The `with_output()` method takes a closure that receives the audio format specification and returns a boxed `AudioDecode` implementation.

### 3. Decoding the File

The main decoding call:

```rust
decode_file_path_str(
    file_path,
    &mut handler,
    true,  // enable_gapless: gapless playback support
    false, // verify: skip verification for faster decoding
    None,  // track_num: auto-select first audio track
    None,  // seek: start from beginning
)?;
```

## Key Concepts

### AudioDecode Trait

The `AudioDecode` trait is the core interface for processing decoded audio:

- **`decoded()`**: Called for each packet of decoded audio. Receives an `AudioBuffer<f32>` containing the samples.
- **`flush()`**: Called when decoding completes, allowing for cleanup and final processing.

### Audio Terminology

- **Frame**: A single sample for all channels (e.g., one frame = left sample + right sample for stereo)
- **Sample**: A single audio value for one channel
- **Channels**: Number of audio channels (1=mono, 2=stereo, etc.)
- **Sample Rate**: Samples per second per channel (e.g., 44100 Hz)

### AudioBuffer

The `AudioBuffer<f32>` contains decoded audio as 32-bit floating-point samples:

- `decoded.frames()`: Number of frames in the buffer
- `decoded.chan(n)`: Get samples for channel `n`
- Samples are normalized floats typically in range [-1.0, 1.0]

### AudioDecodeHandler

The handler coordinates the decoding process:

- Accepts multiple output handlers via `with_output()`
- Supports filters via `with_filter()` for audio processing
- Supports cancellation via `with_cancellation_token()`
- Opens outputs when format is detected
- Routes decoded audio to all registered outputs

## Testing the Example

1. **Try different formats**:

    ```bash
    cargo run --manifest-path packages/audio_decoder/examples/basic_usage/Cargo.toml -- song.mp3
    cargo run --manifest-path packages/audio_decoder/examples/basic_usage/Cargo.toml -- audio.flac
    cargo run --manifest-path packages/audio_decoder/examples/basic_usage/Cargo.toml -- track.opus
    ```

2. **Test with different channel configurations**:
    - Mono files (1 channel)
    - Stereo files (2 channels)
    - Multi-channel files (5.1 surround, etc.)

3. **Verify sample counting**:
    - Check that the reported duration matches the actual file duration
    - Verify sample count = frames × channels

## Troubleshooting

### "File not found" error

- Ensure the file path is correct and accessible
- Use absolute paths if relative paths don't work

### "Unsupported format" error

- The format may not be enabled in the Cargo features
- Check `Cargo.toml` for enabled features (currently: `all-formats`)
- Symphonia may not support the specific codec variant

### No output or truncated output

- Check that the file is not corrupted
- Ensure sufficient disk space and permissions
- Verify the file is a valid audio file with a proper header

### Compile errors

- Ensure you have the latest dependencies: `cargo update`
- Check that workspace dependencies are properly configured

## Related Examples

Currently, this is the only example for `moosicbox_audio_decoder`. Future examples may include:

- Async decoding with `decode_file_path_str_async()`
- Custom media sources with `decode_media_source_async()`
- Audio filtering and effects
- Streaming from remote sources
- Seeking within audio files
- Cancellation and progress tracking
