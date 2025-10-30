# Basic Audio Encoding Example

## Summary

This example demonstrates how to encode PCM audio data to multiple formats (AAC, FLAC, MP3, and Opus) using the `moosicbox_audio_encoder` library. It shows the complete workflow from creating encoders to processing audio samples and handling encoding results.

## What This Example Demonstrates

- Creating encoders for different audio formats (AAC, FLAC, MP3, Opus)
- Generating test PCM audio data in different sample formats (i16, i32, f32)
- Encoding audio samples with proper buffer management
- Handling `EncodeInfo` results to track encoding statistics
- Using feature flags to enable/disable specific codecs
- Proper error handling for encoding operations
- Understanding compression ratios and output sizes

## Prerequisites

- Basic understanding of audio encoding concepts (PCM, codecs, sample rates)
- Familiarity with Rust error handling using `Result` and `?`
- Understanding of audio sample formats (i16 for most codecs, i32 for FLAC, f32 for Opus)

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml
```

To run with specific codecs only:

```bash
# Run with only MP3 encoding
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features mp3

# Run with AAC and Opus
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features "aac,opus"
```

## Expected Output

```
=== MoosicBox Audio Encoder - Basic Encoding Examples ===

This example demonstrates encoding PCM audio to various formats.

--- AAC Encoding Example ---
✓ Created AAC encoder (44.1kHz stereo, VBR Very High, ADTS format)
✓ Generated 2048 PCM samples (i16)
✓ Encoded successfully:
  - Input samples consumed: 2048
  - Output bytes produced: 423
  - Compression ratio: 9.68x

--- FLAC Encoding Example ---
✓ Created FLAC encoder (block size: 512)
✓ Generated 1024 PCM samples (i32)
✓ Encoded successfully:
  - Input samples consumed: 1024
  - Output bytes produced: 182
  - Note: FLAC is lossless compression

--- MP3 Encoding Example ---
✓ Created MP3 encoder (320kbps, 44.1kHz stereo, best quality)
✓ Generated 2048 PCM samples (i16)
✓ Encoded successfully:
  - Input samples consumed: 2048
  - Output bytes produced: 417
  - Output buffer length: 417
  - Compression ratio: 9.83x

--- Opus Encoding Example ---
✓ Created Opus encoder (48kHz stereo)
✓ Generated 1920 PCM samples (f32, 20ms frame)
✓ Encoded successfully:
  - Input samples consumed: 1920
  - Output bytes produced: 183
  - Compression ratio: 41.97x

=== All Examples Completed Successfully ===
```

## Code Walkthrough

### 1. Test Signal Generation

The example generates sine wave test signals in different formats:

```rust
fn generate_test_pcm_i16(sample_count: usize) -> Vec<i16> {
    let sample_rate = 44100.0;
    let frequency = 440.0; // A4 note
    // Generate sine wave samples...
}
```

This creates a 440 Hz tone (musical note A4) for demonstration purposes. Different encoders require different sample formats:

- **i16**: Used by AAC and MP3 encoders
- **i32**: Used by FLAC encoder
- **f32**: Used by Opus encoder

### 2. AAC Encoding

```rust
let encoder = encoder_aac()?;
let input = generate_test_pcm_i16(2048);
let mut output = vec![0u8; 8192];
let info = encode_aac(&encoder, &input, &mut output)?;
```

Creates an AAC encoder with default settings (44.1kHz stereo, VBR very high quality, ADTS format). The output buffer should be approximately 2x the input size for AAC encoding.

### 3. FLAC Encoding

```rust
let mut encoder = encoder_flac()?;
let input = generate_test_pcm_i32(1024);
let mut output = vec![0u8; 8192];
let info = encode_flac(&mut encoder, &input, &mut output)?;
```

FLAC provides lossless compression, preserving original audio quality while reducing file size. Note that the encoder is mutable and uses i32 samples.

### 4. MP3 Encoding

```rust
let mut encoder = encoder_mp3()?;
let input = generate_test_pcm_i16(2048);
let (output, info) = encode_mp3(&mut encoder, &input)?;
```

MP3 encoding returns both the output buffer and encoding info. The encoder automatically manages buffer allocation and includes ID3 tag support.

### 5. Opus Encoding

```rust
let mut encoder = encoder_opus()?;
let input = generate_test_pcm_f32(1920); // 20ms frame at 48kHz stereo
let mut output = vec![0u8; 4096];
let info = encode_opus_float(&mut encoder, &input, &mut output)?;
```

Opus requires specific frame sizes (2.5, 5, 10, 20, 40, or 60 ms). This example uses a 20ms frame (1920 samples at 48kHz stereo).

### 6. Error Handling

```rust
if let Err(e) = encode_aac_example() {
    eprintln!("AAC encoding failed: {e}");
}
```

Each encoder function returns a `Result`, allowing graceful error handling. Errors are caught and displayed without stopping other encoders.

## Key Concepts

### EncodeInfo Structure

All encoders return an `EncodeInfo` struct containing:

- `output_size`: Number of bytes written to the output buffer
- `input_consumed`: Number of input samples consumed from the input buffer

This information is essential for:

- Tracking encoding progress
- Managing buffer offsets for streaming
- Calculating compression ratios
- Debugging encoding issues

### Sample Formats

Different encoders require specific PCM sample formats:

- **i16 (16-bit signed integer)**: AAC, MP3 - Most common format, range -32768 to 32767
- **i32 (32-bit signed integer)**: FLAC - Higher precision for lossless encoding
- **f32 (32-bit floating point)**: Opus - Range typically -1.0 to 1.0

### Buffer Management

Each encoder has different buffer requirements:

- **AAC**: Requires pre-allocated output buffer, typically 2x input size
- **FLAC**: Uses internal streaming buffer, copies to provided buffer
- **MP3**: Returns allocated buffer with exact size needed
- **Opus**: Requires pre-allocated buffer, variable compression

### Feature Flags

The example uses conditional compilation to support different codec combinations:

```rust
#[cfg(feature = "aac")]
fn encode_aac_example() -> Result<(), Box<dyn std::error::Error>> {
    // AAC-specific code
}
```

This allows building with only the required codecs, reducing binary size and dependencies.

## Testing the Example

### Verify All Codecs Work

Run with default features to test all encoders:

```bash
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml
```

You should see output sections for AAC, FLAC, MP3, and Opus, each showing successful encoding.

### Test Individual Codecs

Test each codec independently:

```bash
# Test AAC only
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features aac

# Test FLAC only
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features flac

# Test MP3 only
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features mp3

# Test Opus only
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features --features opus
```

### Verify Compilation Without Features

Ensure the example handles the case where no features are enabled:

```bash
cargo run --manifest-path packages/audio_encoder/examples/basic_encoding/Cargo.toml --no-default-features
```

This should compile but display an error message about no encoders being available.

## Troubleshooting

### Compilation Errors

**Problem**: Native library dependencies not found (e.g., `fdk-aac`, `lame`, `opus`)

**Solution**: These are typically provided by the workspace dependencies. If you encounter issues, ensure you're running from the repository root and the required system libraries are installed. For most platforms, the Rust crates bundle or build these libraries automatically.

**Problem**: Feature compilation errors

**Solution**: Check that you're using compatible feature combinations. All features can be enabled together, but ensure your command syntax is correct when using `--features`.

### Runtime Issues

**Problem**: Buffer too small errors

**Solution**: The example uses conservative buffer sizes. If you modify the input sizes, ensure output buffers are adequately sized:

- AAC: ~2x input size
- FLAC: ~2x input size (for worst-case incompressible data)
- MP3: Uses automatic buffer allocation
- Opus: ~4KB for typical frames

**Problem**: Opus frame size errors

**Solution**: Opus requires specific frame sizes. Valid frame durations are 2.5, 5, 10, 20, 40, or 60 ms. At 48kHz stereo, these correspond to 240, 480, 960, 1920, 3840, or 5760 samples.

### Encoder Initialization Failures

**Problem**: Encoder creation fails with configuration errors

**Solution**: The example uses default settings that should work on all platforms. If you modify encoder parameters, consult each codec's documentation for valid parameter ranges.

## Related Examples

Currently, this is the only example for `moosicbox_audio_encoder`. Future examples might include:

- Streaming audio encoding from files
- Real-time audio encoding
- Ogg/Opus container usage
- Advanced encoder configuration
- Format conversion workflows
