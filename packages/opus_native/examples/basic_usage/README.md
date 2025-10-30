# Basic Usage Example

This example demonstrates the fundamental usage patterns of the `moosicbox_opus_native` pure Rust Opus decoder.

## Summary

A comprehensive introduction to decoding Opus audio packets using the SILK, CELT, and Hybrid decoder modes with various sample rates and channel configurations.

## What This Example Demonstrates

- Creating decoder instances with different sample rates and channel configurations
- Decoding CELT-only packets (optimized for music/fullband audio)
- Decoding SILK-only packets (optimized for speech/narrowband audio)
- Handling packet loss with Packet Loss Concealment (PLC)
- Understanding supported sample rates and frame sizes
- Working with feature flags to enable/disable codec modes
- Proper buffer allocation for decoded PCM output

## Prerequisites

- Basic understanding of digital audio concepts (sample rate, channels, PCM)
- Familiarity with the Opus codec format (recommended but not required)
- Rust development environment

## Running the Example

```bash
# Run with default features (SILK + CELT enabled)
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml

# Run with only CELT decoder
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml --no-default-features --features celt

# Run with only SILK decoder
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml --no-default-features --features silk

# Run with all features including resampling
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml --features resampling
```

## Expected Output

When you run the example, you should see output similar to:

```
=== MoosicBox Opus Native - Basic Usage Example ===

Example 1: CELT-only decoding (48kHz stereo)
---------------------------------------------------
✓ Decoded 480 samples per channel
  Buffer size: 960 samples (480 L/R pairs)
  Sample range: -32768 to 32767
  First 10 sample pairs: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]

Example 2: SILK-only decoding (16kHz mono)
---------------------------------------------------
✓ Decoded 320 samples per channel
  Buffer size: 320 samples
  First 10 samples: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]

Example 3: Packet Loss Concealment
---------------------------------------------------
✓ Generated 480 concealment samples
  (PLC fills in missing audio during packet loss)

Example 4: Supported Sample Rates
---------------------------------------------------
The Opus codec supports the following output sample rates:
  • 8kHz  (Narrowband)     - Phone quality
  • 12kHz (Mediumband)     - Enhanced speech
  • 16kHz (Wideband)       - High-quality speech
  • 24kHz (Super-wideband) - High-fidelity speech
  • 48kHz (Fullband)       - Music quality

Frame sizes can be 2.5ms, 5ms, 10ms, 20ms, 40ms, or 60ms.
Most common frame size is 20ms for speech, 10ms for music.

Example 5: Decoder Capabilities
---------------------------------------------------
✓ SILK decoder enabled  - Speech/narrowband
✓ CELT decoder enabled  - Music/fullband
✓ Hybrid mode enabled   - SILK+CELT combined
✗ Resampling disabled   - Output rate must match internal rate

=== Example Complete ===

Key Takeaways:
1. Create a decoder with Decoder::new(sample_rate, channels)
2. Allocate output buffer with correct size for frame duration
3. Call decode(Some(&packet), &mut output, false) to decode
4. Use decode(None, ...) to handle packet loss with PLC
5. Enable features (silk, celt, hybrid, resampling) as needed
```

## Code Walkthrough

### 1. Creating a Decoder

The first step is to create a decoder instance with your desired output configuration:

```rust
use moosicbox_opus_native::{Decoder, SampleRate, Channels};

// Create a decoder for 48kHz stereo output
let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;
```

The `SampleRate` enum supports:

- `Hz8000` - Narrowband (phone quality)
- `Hz12000` - Mediumband (enhanced speech)
- `Hz16000` - Wideband (high-quality speech)
- `Hz24000` - Super-wideband (high-fidelity speech)
- `Hz48000` - Fullband (music quality)

The `Channels` enum supports `Mono` (1 channel) and `Stereo` (2 channels).

### 2. Allocating Output Buffers

Buffer size depends on frame duration and sample rate:

```rust
// Example: 10ms frame at 48kHz stereo
// 10ms × 48000 Hz / 1000 = 480 samples per channel
// Stereo = 480 × 2 = 960 total samples (interleaved L/R)
let mut output = vec![0i16; 480 * 2];
```

Common calculations:

- **10ms @ 48kHz mono**: 480 samples
- **10ms @ 48kHz stereo**: 960 samples
- **20ms @ 16kHz mono**: 320 samples
- **20ms @ 16kHz stereo**: 640 samples

### 3. Decoding Packets

The `decode()` method processes Opus packets into PCM audio:

```rust
// Decode an Opus packet
let samples = decoder.decode(Some(&packet), &mut output, false)?;

// samples = number of samples per channel decoded
// output = interleaved PCM samples (L/R/L/R... for stereo)
```

Parameters:

- `input`: `Option<&[u8]>` - The Opus packet to decode, or `None` for packet loss
- `output`: `&mut [i16]` - Output buffer for decoded PCM samples
- `fec`: `bool` - Forward Error Correction flag (set to `false` for normal decoding)

### 4. Handling Packet Loss

When a packet is lost, pass `None` to trigger Packet Loss Concealment:

```rust
// Generate concealment audio for a lost packet
let samples = decoder.decode(None, &mut output, false)?;
```

The decoder will generate appropriate filler audio (currently silence, full PLC coming in future phases).

### 5. Understanding TOC Bytes

Each Opus packet starts with a TOC (Table of Contents) byte that encodes:

- **Bits 0-4**: Configuration (determines mode, bandwidth, frame size)
- **Bit 5**: Stereo flag (0 = mono, 1 = stereo)
- **Bits 6-7**: Frame count code

Example TOC bytes used in this example:

- `0x7C` - CELT-only, 48kHz, 10ms frame, stereo
- `0x44` - SILK-only, 16kHz (Wideband), 20ms frame, mono

### 6. Feature Flags

The crate uses Cargo features to enable/disable codec modes:

- `silk` - SILK decoder for speech (8kHz, 12kHz, 16kHz internal rates)
- `celt` - CELT decoder for music (48kHz internal rate)
- `hybrid` - Combined SILK+CELT decoder (requires both `silk` and `celt`)
- `resampling` - Automatic sample rate conversion (requires `symphonia` and `moosicbox_resampler`)

Without the appropriate feature, attempting to decode a packet will return an `UnsupportedMode` error.

## Key Concepts

### Opus Decoder Modes

Opus uses three decoder modes depending on the content:

1. **SILK-only**: Optimized for speech, operates at 8/12/16 kHz internally
2. **CELT-only**: Optimized for music, operates at 48 kHz internally
3. **Hybrid**: SILK for low frequencies (0-8kHz) + CELT for high frequencies (8-20kHz)

The mode is automatically determined by the TOC byte in each packet.

### Sample Rates and Resampling

- **Internal rate**: The rate at which the codec processes audio
    - SILK: 8kHz, 12kHz, or 16kHz (depends on bandwidth)
    - CELT: Always 48kHz
- **Output rate**: The rate specified when creating the decoder

When these don't match:

- **With resampling feature**: Automatic conversion to output rate
- **Without resampling feature**: Returns an error

### Frame Sizes

Opus supports variable frame sizes from 2.5ms to 60ms:

- **2.5ms, 5ms**: Ultra-low latency (rarely used)
- **10ms**: Common for music (low latency, good quality)
- **20ms**: Most common for speech (good balance)
- **40ms, 60ms**: Higher latency, better efficiency for speech

### PCM Output Format

Decoded audio is 16-bit signed PCM (`i16`):

- **Range**: -32768 to 32767
- **Mono**: Sequential samples `[s0, s1, s2, ...]`
- **Stereo**: Interleaved samples `[L0, R0, L1, R1, L2, R2, ...]`

## Testing the Example

### Experimenting with Features

Try running with different feature combinations to see how the example adapts:

```bash
# CELT-only: Only examples 1, 3, and 4-5 will run
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml \
  --no-default-features --features celt

# SILK-only: Only examples 2, 4, and 5 will run
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml \
  --no-default-features --features silk

# No decoders: Examples will show warnings about missing features
cargo run --manifest-path packages/opus_native/examples/basic_usage/Cargo.toml \
  --no-default-features
```

### Modifying the Example

Try these modifications to learn more:

1. **Change sample rates**: Modify `SampleRate::Hz48000` to other rates
2. **Change frame sizes**: Adjust buffer sizes for different frame durations
3. **Add stereo/mono conversion**: Process the interleaved output
4. **Save to file**: Write the PCM output to a `.raw` or `.wav` file

## Troubleshooting

### "Unsupported mode" error

**Problem**: Decoder returns `Error::UnsupportedMode`

**Solution**: Enable the required feature flag:

- For SILK packets: `--features silk`
- For CELT packets: `--features celt`
- For Hybrid packets: `--features hybrid` (or both `silk` and `celt`)

### "Output buffer too small" error

**Problem**: Decoder returns `Error::InvalidPacket("Output buffer too small")`

**Solution**: Calculate correct buffer size:

```
buffer_size = (frame_duration_ms × sample_rate_hz / 1000) × num_channels
```

Example: 20ms @ 16kHz stereo = (20 × 16000 / 1000) × 2 = 640 samples

### "Resampling not available" error

**Problem**: Decoder returns `Error::InvalidSampleRate("Resampling not available...")`

**Solution**: Either:

- Enable resampling feature: `--features resampling`
- Match decoder rate to packet's internal rate (e.g., use `Hz16000` for SILK WB)

### Packets decode to silence

**Problem**: Output buffer contains all zeros

**Explanation**: In this example, we're using dummy packets (filled with a single byte value) rather than real Opus bitstreams. Real Opus packets from an encoder would produce actual audio. This example focuses on demonstrating the API usage patterns.

## Related Examples

- `packages/audio_decoder/examples/basic_usage` - Higher-level audio decoding with multiple codec support
- `packages/hyperchad/examples/details_summary` - Web component usage patterns
- `packages/async/examples/cancel` - Async runtime patterns for audio processing

For more information about the Opus codec, see [RFC 6716](https://datatracker.ietf.org/doc/html/rfc6716).
