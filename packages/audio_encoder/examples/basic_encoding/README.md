# Basic Audio Encoding Example

This example demonstrates how to use the `moosicbox_audio_encoder` package to encode PCM audio data into various formats including AAC, FLAC, MP3, and Opus.

## Summary

A comprehensive example showing the basic workflow for encoding audio with all four supported formats. Each encoder is demonstrated with appropriate PCM sample types and buffer handling patterns.

## What This Example Demonstrates

- Creating encoders for AAC, FLAC, MP3, and Opus formats
- Encoding PCM audio samples with each format
- Handling different PCM sample types (i16, i32, f32) required by each encoder
- Working with the `EncodeInfo` result structure
- Understanding output buffer management patterns for each encoder
- Comparing compression ratios across different formats

## Prerequisites

- Basic understanding of PCM audio data
- Familiarity with audio sample formats and bit depths
- Knowledge of Rust's `Result` type for error handling

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml
```

To run with only specific encoders enabled:

```bash
# AAC and MP3 only
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml \
  --no-default-features \
  --features aac,mp3

# FLAC only (lossless compression)
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml \
  --no-default-features \
  --features flac
```

## Expected Output

When you run the example with all features enabled, you should see output like:

```
MoosicBox Audio Encoder - Basic Encoding Example
=================================================

=== AAC Encoding ===
  Input samples: 2048
  Samples consumed: 2048
  Bytes encoded: 512
  Compression ratio: 8.00x

=== FLAC Encoding ===
  Input samples: 1024
  Samples consumed: 1024
  Bytes encoded: 256
  Note: FLAC is lossless compression

=== MP3 Encoding ===
  Input samples: 2304
  Samples consumed: 2304
  Bytes encoded: 418
  Compression ratio: 11.02x

=== Opus Encoding ===
  Input samples: 1920
  Samples consumed: 1920
  Bytes encoded: 64
  Compression ratio: 120.00x

=== Summary ===
All enabled encoders demonstrated successfully!

Key differences:
  - AAC/MP3: Use i16 PCM samples
  - FLAC: Uses i32 PCM samples (lossless)
  - Opus: Uses f32 PCM samples in range [-1.0, 1.0]
  - MP3: Returns owned output buffer
  - AAC/FLAC/Opus: Write to provided buffer
```

Note: The exact byte counts and compression ratios will vary as these depend on the encoder implementations and the audio content (this example uses silent audio).

## Code Walkthrough

### AAC Encoding

AAC encoding uses the `fdk-aac` library and requires i16 PCM samples:

```rust
use moosicbox_audio_encoder::aac::{encoder_aac, encode_aac};

// Create encoder (44.1kHz stereo, ADTS format)
let encoder = encoder_aac()?;

// Prepare input (i16 samples) and output buffer
let input_samples: Vec<i16> = vec![0; 2048];
let mut output_buffer = vec![0u8; 8192];

// Encode and get info
let encode_info = encode_aac(&encoder, &input_samples, &mut output_buffer)?;
println!("Encoded {} bytes", encode_info.output_size);
```

The encoder is configured for MPEG-4 Low Complexity AAC with very high variable bitrate.

### FLAC Encoding

FLAC is a lossless codec that uses i32 samples to support higher bit depths:

```rust
use moosicbox_audio_encoder::flac::{encoder_flac, encode_flac};

// Create encoder (block size 512)
let mut encoder = encoder_flac()?;

// Prepare input (i32 samples) and output buffer
let input_samples: Vec<i32> = vec![0; 1024];
let mut output_buffer = vec![0u8; 8192];

// Encode and get info
let encode_info = encode_flac(&mut encoder, &input_samples, &mut output_buffer)?;
```

FLAC encoding is stateful and requires a mutable encoder reference.

### MP3 Encoding

MP3 encoding uses the LAME encoder and has a unique API that returns the output buffer:

```rust
use moosicbox_audio_encoder::mp3::{encoder_mp3, encode_mp3};

// Create encoder (320kbps, 44.1kHz stereo)
let mut encoder = encoder_mp3()?;

// Prepare input (i16 samples)
let input_samples: Vec<i16> = vec![0; 2304];

// Encode - returns tuple of (output buffer, encode info)
let (output_buffer, encode_info) = encode_mp3(&mut encoder, &input_samples)?;
println!("Created {} bytes of MP3 data", output_buffer.len());
```

Note that unlike other encoders, MP3's `encode_mp3()` function allocates and returns the output buffer rather than writing to a provided buffer.

### Opus Encoding

Opus uses floating-point samples in the range [-1.0, 1.0]:

```rust
use moosicbox_audio_encoder::opus::{encoder_opus, encode_opus_float};

// Create encoder (48kHz stereo)
let mut encoder = encoder_opus()?;

// Prepare input (f32 samples in range [-1.0, 1.0]) and output buffer
let input_samples: Vec<f32> = vec![0.0; 1920];
let mut output_buffer = vec![0u8; 4000];

// Encode and get info
let encode_info = encode_opus_float(&mut encoder, &input_samples, &mut output_buffer)?;
```

Opus is designed for real-time applications and typically uses smaller frame sizes (20ms frames at 48kHz = 960 samples per channel, or 1920 for stereo).

### Understanding EncodeInfo

All encoders return an `EncodeInfo` struct:

```rust
pub struct EncodeInfo {
    pub output_size: usize,      // Bytes written to output buffer
    pub input_consumed: usize,   // Input samples consumed
}
```

This allows you to:

- Know how much of the output buffer contains valid encoded data
- Track how many input samples were processed
- Handle partial encoding when input doesn't align with encoder frame requirements

## Key Concepts

### Sample Type Requirements

Each encoder expects a specific PCM sample type:

- **AAC** (`i16`): Signed 16-bit integers, typical for most audio applications
- **FLAC** (`i32`): Signed 32-bit integers, supports higher bit depths for lossless encoding
- **MP3** (`i16`): Signed 16-bit integers, standard for lossy compression
- **Opus** (`f32`): Floating-point values in range [-1.0, 1.0], normalized audio

### Encoder Configuration

All encoders use sensible defaults:

- **AAC**: 44.1kHz stereo, VBR very high quality, ADTS transport
- **FLAC**: 44.1kHz stereo, 16-bit, 512-sample block size
- **MP3**: 44.1kHz stereo, 320kbps CBR, best quality
- **Opus**: 48kHz stereo, audio application mode

### Buffer Management

Two buffer management patterns are used:

1. **Pre-allocated buffer** (AAC, FLAC, Opus): You provide an output buffer, and the encoder writes to it. The `EncodeInfo.output_size` tells you how many bytes were written.

2. **Returned buffer** (MP3): The encoder allocates and returns the output buffer, ensuring it's sized correctly for the encoded data.

### Compression Characteristics

- **Lossless** (FLAC): Preserves original audio quality, moderate compression
- **Lossy** (AAC, MP3, Opus): Discards inaudible information for higher compression
- **Real-time optimized** (Opus): Designed for low-latency streaming applications
- **High quality** (AAC, MP3): Optimized for music and audio storage

## Testing the Example

The example uses silent audio (all zeros) for simplicity. To test with real audio:

1. Replace the zero-filled vectors with actual PCM data
2. Ensure sample counts match encoder requirements:
    - AAC: Multiples of 1024 samples per channel
    - FLAC: Any length (block size is 512)
    - MP3: Multiples of 1152 samples per channel
    - Opus: Multiples of frame size (typically 960 samples per channel for 20ms at 48kHz)

## Troubleshooting

### "Buffer too small" errors

- Increase output buffer size
- For MP3, this is handled automatically
- For AAC/FLAC/Opus, allocate at least 2x the input size

### "Invalid input size" errors

- Ensure input sample count is appropriate for the encoder
- Check that you're using the correct sample type (i16 vs i32 vs f32)
- Verify you're providing stereo-interleaved samples where expected

### Feature compilation errors

- Ensure you have the required features enabled in `Cargo.toml`
- Each format requires its respective feature flag: `aac`, `flac`, `mp3`, `opus`

## Related Examples

This is currently the only example for `moosicbox_audio_encoder`. Future examples might include:

- Streaming encoding with file I/O
- Real-time audio encoding pipeline
- Format conversion workflows
- Opus Ogg container usage (using `OpusWrite`)
