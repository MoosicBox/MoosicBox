# Basic Audio Resampling Example

A comprehensive example demonstrating audio sample rate conversion using the MoosicBox resampler.

## Summary

This example shows how to use `moosicbox_resampler` to convert audio from one sample rate to another (44.1kHz to 48kHz). It demonstrates the complete workflow: creating a resampler, processing audio chunks, handling buffering, and flushing remaining samples.

## What This Example Demonstrates

- Creating a `Resampler` instance with specific input and output sample rates
- Configuring signal specifications for stereo audio
- Generating synthetic audio data in Symphonia `AudioBuffer` format
- Processing audio in fixed-size chunks
- Understanding the resampler's buffering behavior
- Handling the interleaved output format
- Flushing remaining samples at the end of a stream
- Verifying the conversion ratio matches expectations

## Prerequisites

- Basic understanding of digital audio concepts (sample rates, channels, frames)
- Familiarity with Rust's `Result` type and error handling
- Knowledge of Symphonia's `AudioBuffer` and `SignalSpec` types is helpful but not required

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/resampler/examples/basic_resampling/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/resampler/examples/basic_resampling
cargo run
```

## Expected Output

```
=== MoosicBox Resampler: Basic Resampling Example ===

Input Configuration:
  Sample Rate: 44100 Hz
  Channels: 2 (stereo)
  Chunk Size: 1024 samples

Output Configuration:
  Sample Rate: 48000 Hz

Created resampler: 44100Hz -> 48000Hz

Processing chunk 1/5:
  Input frames: 1024
  Output samples (interleaved): 2224
  Output frames: 1112
  Sample values (mid): L=0.2891, R=0.2847

Processing chunk 2/5:
  Input frames: 1024
  Output samples (interleaved): 2224
  Output frames: 1112
  Sample values (mid): L=-0.1245, R=-0.1198

...

Flushing remaining samples...
  Flushed 2224 final samples

=== Summary ===
Total input samples processed: 10240
Total output samples produced: 11120
Conversion ratio: 1.0859
Expected ratio: 1.0884
```

The conversion ratio should be approximately 1.088 (48000/44100), indicating successful resampling.

## Code Walkthrough

### 1. Creating the Signal Specification

```rust
const INPUT_SAMPLE_RATE: u32 = 44100;
const OUTPUT_SAMPLE_RATE: usize = 48000;
const CHUNK_DURATION: u64 = 1024;

let channels = Channels::FRONT_LEFT | Channels::FRONT_RIGHT;
let input_spec = SignalSpec::new(INPUT_SAMPLE_RATE, channels);
```

The `SignalSpec` defines the audio format: sample rate and channel configuration. Here we're using stereo (left + right channels) at CD quality (44.1kHz).

### 2. Creating the Resampler

```rust
let mut resampler: Resampler<f32> =
    Resampler::new(input_spec, OUTPUT_SAMPLE_RATE, CHUNK_DURATION);
```

The resampler is configured to:

- Accept input matching `input_spec` (44.1kHz stereo)
- Output at 48kHz
- Process in chunks of 1024 frames
- Use `f32` samples (generic parameter)

The `CHUNK_DURATION` determines the resampler's internal buffer size. It must accumulate at least this many frames before producing output.

### 3. Processing Audio Chunks

```rust
let audio_buffer = generate_test_audio(input_spec, CHUNK_DURATION);

if let Some(resampled_samples) = resampler.resample(&audio_buffer) {
    println!("Output samples (interleaved): {}", resampled_samples.len());
    // Use the resampled audio...
} else {
    println!("(Buffering - need more samples before output)");
}
```

The `resample()` method:

- Takes a planar (non-interleaved) `AudioBuffer` as input
- Returns `Some(&[f32])` with interleaved samples when enough data is buffered
- Returns `None` if more input is needed before producing output

**Interleaved format**: Samples are arranged as `[L, R, L, R, ...]` for stereo.

### 4. Flushing Remaining Samples

```rust
if let Some(final_samples) = resampler.flush() {
    println!("Flushed {} final samples", final_samples.len());
}
```

At the end of a stream, call `flush()` to process any remaining buffered samples. This pads the internal buffer with silence to meet the required chunk size and produces final output.

### 5. Generating Test Audio

The `generate_test_audio()` function creates a synthetic sine wave at 440 Hz (A4 note):

```rust
fn generate_test_audio(spec: SignalSpec, duration: u64) -> AudioBuffer<f32> {
    let mut buffer = AudioBuffer::new(duration, spec);
    buffer.render_reserved(Some(frames));

    // Fill with sine wave data
    for channel_idx in 0..spec.channels.count() {
        let channel = buffer.chan_mut(channel_idx);
        for (frame_idx, sample) in channel.iter_mut().enumerate() {
            let time = frame_idx as f32 / sample_rate;
            let angle = 2.0 * PI * frequency * time;
            *sample = (0.3 * angle.sin()).into_sample();
        }
    }

    buffer
}
```

This demonstrates how to create and populate a Symphonia `AudioBuffer` in planar format (each channel is a separate array).

## Key Concepts

### Sample Rate Conversion

Sample rate conversion changes the number of samples per second in an audio signal. Converting from 44.1kHz to 48kHz increases the sample count by approximately 8.8% (48000/44100 ≈ 1.088).

### Fixed-Size Processing

The resampler uses fixed-size input chunks (`CHUNK_DURATION`). This provides:

- **Predictable latency**: Output is produced after each complete chunk
- **Efficient processing**: Batch processing reduces overhead
- **Buffering behavior**: Partial chunks are held until enough data arrives

### Planar vs. Interleaved

- **Planar** (input): Each channel's samples are stored separately: `[L1, L2, L3, ...], [R1, R2, R3, ...]`
- **Interleaved** (output): Channels are mixed together: `[L1, R1, L2, R2, L3, R3, ...]`

Symphonia uses planar `AudioBuffer` internally, while many audio APIs expect interleaved output.

### Buffering and Output Timing

The resampler may not produce output on every call to `resample()`:

- First call: Returns `None` (needs to fill internal buffer)
- Subsequent calls: Return `Some(...)` with resampled data
- Last chunk: Call `flush()` to process remaining samples

## Testing the Example

### Verify the Conversion Ratio

The summary at the end shows:

```
Conversion ratio: 1.0859
Expected ratio: 1.0884
```

These should be close (within ~2%), confirming the resampler is working correctly. Small differences are due to:

- Buffering effects
- Rounding in sample counts
- The flush operation padding

### Modify Parameters

Try changing the constants to experiment:

```rust
// Try different sample rates
const OUTPUT_SAMPLE_RATE: usize = 96000; // High-res audio

// Try different chunk sizes
const CHUNK_DURATION: u64 = 2048; // Larger chunks = less frequent output

// Try more/fewer chunks
const NUM_CHUNKS: usize = 10;
```

### Add Mono Audio

Modify to use mono instead of stereo:

```rust
let channels = Channels::FRONT_LEFT;
```

## Troubleshooting

### "Buffer too small" or similar errors

Ensure `CHUNK_DURATION` is reasonable (typically 512-4096 samples). Very small values may cause issues with the FFT-based resampler.

### Output sample count doesn't match expectations

Remember:

- The first chunk(s) may not produce output (buffering)
- The flush operation adds padding
- Sample counts are per-channel for frames, total for samples

### Compilation errors about trait bounds

Ensure you're using a sample type that implements the required traits:

```rust
T: Sample + ReversibleSample<f32>
```

Common types that work: `f32`, `f64`, `i16`, `i32`

## Related Examples

This is currently the only example for `moosicbox_resampler`. For related audio processing examples, see:

- `moosicbox_audio_decoder` - Decoding audio files to AudioBuffer format
- `moosicbox_audio_encoder` - Encoding AudioBuffer to various formats
- `moosicbox_audio_output` - Playing resampled audio through output devices
