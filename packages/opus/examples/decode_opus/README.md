# Opus Audio Decoding Example

This example demonstrates how to decode Opus audio files using the `moosicbox_opus` codec with the Symphonia multimedia framework.

## Summary

This example shows the complete workflow for decoding Opus audio: registering the codec, opening a media file, probing the format, creating a decoder, and processing packets to extract decoded audio samples.

## What This Example Demonstrates

- Registering the Opus codec with Symphonia's codec registry
- Opening and probing Opus audio files
- Finding and selecting the Opus audio track
- Creating and configuring an `OpusDecoder`
- Decoding Opus packets into audio samples
- Accessing decoded audio buffer information (frames, channels, sample rate)
- Handling decoding errors gracefully
- Calculating audio duration from decoded frames

## Prerequisites

- Basic understanding of audio codecs and containers
- Familiarity with the Opus audio codec (RFC 6716)
- Knowledge of Rust error handling with `Result` and `?` operator
- An Opus audio file to test with (`.opus` or `.ogg` with Opus codec)

You can create test Opus files using tools like `opusenc` from the opus-tools package:

```bash
# Convert a WAV file to Opus
opusenc input.wav output.opus
```

## Running the Example

```bash
cargo run --manifest-path packages/opus/examples/decode_opus/Cargo.toml -- /path/to/audio.opus
```

Replace `/path/to/audio.opus` with the path to any Opus audio file. Both `.opus` files and `.ogg` files containing Opus-encoded audio are supported.

## Expected Output

```
Decoding Opus file: /path/to/audio.opus

Track information:
  Codec: Opus
  Sample rate: 48000 Hz
  Channels: 2
  Duration: 180.50 seconds

Decoding packets (. = 100 packets):
...............................

Decoding complete!
  Total packets: 2886
  Total frames decoded: 8653920
  Total samples: 17307840
  Actual duration: 180.29 seconds
```

The output shows:

- **Track information**: Sample rate, channel count, and estimated duration from container metadata
- **Progress dots**: One dot for every 100 packets decoded
- **Statistics**: Total packets processed, frames decoded, samples produced, and actual duration

## Code Walkthrough

### 1. Registering the Opus Codec

The first step is to register the Opus codec with Symphonia's codec registry:

```rust
// Get Symphonia's default codec registry
let mut codec_registry = symphonia::default::get_codecs();

// Register the moosicbox_opus codec
moosicbox_opus::register_opus_codec(&mut codec_registry);
```

The `register_opus_codec()` function adds the `OpusDecoder` to the registry, allowing Symphonia to recognize and decode Opus audio streams.

### 2. Opening and Probing the Media File

Next, open the file and probe it to detect the format:

```rust
// Open the file as a media source stream
let file = Box::new(File::open(file_path)?);
let mss = MediaSourceStream::new(file, Default::default());

// Create a hint to help format detection
let mut hint = Hint::new();
hint.with_extension("opus");

// Probe the media source to determine the format
let probed = symphonia::default::get_probe().format(
    &hint,
    mss,
    &format_opts,
    &metadata_opts,
)?;

let mut format: Box<dyn FormatReader> = probed.format;
```

The probe identifies the container format (e.g., Ogg) and provides a `FormatReader` for accessing packets.

### 3. Finding the Opus Audio Track

Opus files may contain multiple tracks. We need to find the audio track:

```rust
let track = format
    .tracks()
    .iter()
    .find(|t| t.codec_params.codec == CODEC_TYPE_OPUS)
    .ok_or("No Opus audio track found")?;

let track_id = track.id;
```

We search for a track with `CODEC_TYPE_OPUS` and save its ID for filtering packets.

### 4. Creating the Decoder

Create an `OpusDecoder` instance using the codec registry:

```rust
let decoder_opts = DecoderOptions::default();
let mut decoder = codec_registry.make(&track.codec_params, &decoder_opts)?;
```

The decoder is initialized with the track's codec parameters (sample rate, channels, etc.).

### 5. Decoding Packets

The main decoding loop reads packets and decodes them:

```rust
loop {
    // Get the next packet
    let packet = match format.next_packet() {
        Ok(packet) => packet,
        Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            break; // End of stream
        }
        Err(e) => return Err(e.into()),
    };

    // Only process packets from our track
    if packet.track_id() != track_id {
        continue;
    }

    // Decode the packet
    match decoder.decode(&packet) {
        Ok(decoded) => {
            // Access decoded audio data
            let frames = decoded.frames();
            let channels = decoded.spec().channels.count();
            // Process samples...
        }
        Err(e) => {
            // Handle decode errors
            eprintln!("Decode error: {}", e);
        }
    }
}
```

Each decoded buffer contains audio samples as floating-point values.

### 6. Accessing Decoded Audio Data

The decoded audio is provided as an `AudioBufferRef`:

```rust
let decoded: AudioBufferRef = decoder.decode(&packet)?;

// Get audio specifications
let spec = decoded.spec();
let sample_rate = spec.rate;
let channel_count = spec.channels.count();

// Get number of frames in this buffer
let frames = decoded.frames();

// Access channel data (match on buffer type)
match decoded {
    AudioBufferRef::F32(buf) => {
        let left_channel = buf.chan(0);   // Left channel samples
        let right_channel = buf.chan(1);  // Right channel samples
        // Each channel contains `frames` samples
    }
    _ => { /* Handle other formats */ }
}
```

## Key Concepts

### Opus Codec

Opus is a versatile audio codec designed for interactive speech and music transmission over the Internet (RFC 6716). Key characteristics:

- **Sample rates**: 8, 12, 16, 24, or 48 kHz
- **Channels**: Mono or stereo (higher channel counts through mapping families)
- **Frame sizes**: Typically 2.5, 5, 10, 20, 40, or 60 ms
- **Bit rates**: Highly variable, typically 6-510 kbps
- **Latency**: Very low, suitable for real-time applications

### RFC 6716 Packet Structure

The `moosicbox_opus` crate implements RFC 6716 packet parsing:

- **TOC byte**: Table of Contents specifying mode, bandwidth, and frame packing
- **Frame packing**: Code 0 (single frame), Code 1 (two equal frames), Code 2 (two VBR frames), Code 3 (multiple frames)
- **Padding**: Optional padding bytes in code 3 packets
- **DTX frames**: Discontinuous Transmission (silence) frames

### Symphonia Integration

Symphonia is a multimedia framework that provides:

- **Format demuxing**: Extracts packets from containers (Ogg, Matroska, etc.)
- **Codec registry**: Plugin system for audio codecs
- **Metadata parsing**: Extract tags, cover art, and format information
- **Seeking**: Random access within media files

The `moosicbox_opus::OpusDecoder` implements the `Decoder` trait to integrate with Symphonia.

### Audio Terminology

- **Packet**: A chunk of compressed audio data from the container
- **Frame**: A decoded audio frame containing samples for all channels at a specific time
- **Sample**: A single audio value for one channel at one point in time
- **Sample rate**: Samples per second per channel (e.g., 48000 Hz)
- **Channels**: Number of audio channels (1=mono, 2=stereo)

### Decoding Pipeline

```
File → MediaSourceStream → FormatReader → Packets → Decoder → AudioBuffer
```

1. **MediaSourceStream**: Provides byte-level I/O
2. **FormatReader**: Demuxes container format into packets
3. **Decoder**: Decodes compressed packets into PCM audio
4. **AudioBuffer**: Contains decoded samples ready for processing or playback

## Testing the Example

### 1. Try Different Opus Files

Test with various Opus file types:

```bash
# Standard Opus file
cargo run --manifest-path packages/opus/examples/decode_opus/Cargo.toml -- music.opus

# Ogg container with Opus codec
cargo run --manifest-path packages/opus/examples/decode_opus/Cargo.toml -- podcast.ogg

# Speech-optimized Opus
cargo run --manifest-path packages/opus/examples/decode_opus/Cargo.toml -- voice.opus
```

### 2. Test Different Channel Configurations

- **Mono files** (1 channel): Voice recordings, podcasts
- **Stereo files** (2 channels): Music, sound effects

### 3. Verify Sample Counts

The relationship between packets, frames, and samples:

- **Frames = sample_rate × duration** (for one channel)
- **Samples = frames × channels** (total across all channels)

For example, a 10-second stereo file at 48 kHz:

- Frames: 480,000
- Samples: 960,000

### 4. Performance Testing

Test with large files to verify performance:

```bash
time cargo run --release --manifest-path packages/opus/examples/decode_opus/Cargo.toml -- large_file.opus
```

Opus decoding is typically very fast, capable of decoding much faster than real-time on modern hardware.

## Troubleshooting

### "No Opus audio track found"

**Cause**: The file does not contain an Opus-encoded audio stream.

**Solutions**:

- Verify the file is actually Opus-encoded (use `ffprobe` or `mediainfo`)
- Check that the file is not corrupted
- Ensure the container format is supported by Symphonia (Ogg, Matroska/WebM)

### "File not found" error

**Solutions**:

- Check the file path is correct
- Use absolute paths if relative paths don't work
- Verify file permissions allow reading

### "Unsupported sample rate" error

**Cause**: The Opus stream uses a sample rate not supported by libopus (only 8, 12, 16, 24, 48 kHz are valid).

**Solutions**:

- This indicates a malformed or non-standard Opus file
- Re-encode the file with a standard Opus encoder

### Decode errors during playback

**Cause**: Corrupted packets or malformed Opus frames.

**Solutions**:

- Check if the file is corrupted (try playing in other applications)
- Re-download or re-encode the file
- The example continues decoding after errors, counting them in the summary

### Compilation errors

**Solutions**:

- Ensure workspace dependencies are up to date: `cargo update`
- Check that the `symphonia` and `moosicbox_opus` versions are compatible
- Run `cargo clean` and rebuild if you encounter cache issues

## Related Examples

### In the moosicbox_opus package

Currently, this is the only example for `moosicbox_opus`.

### Related packages

- **[moosicbox_audio_decoder](../../audio_decoder/examples/basic_usage/README.md)**: Higher-level audio decoding API that uses this codec
- **moosicbox_flac**: Similar codec implementation for FLAC audio
- **moosicbox_vorbis**: Similar codec implementation for Vorbis audio

### External resources

- [RFC 6716 - Opus Codec Specification](https://tools.ietf.org/html/rfc6716)
- [Symphonia Documentation](https://docs.rs/symphonia/)
- [Opus Codec Website](https://opus-codec.org/)
