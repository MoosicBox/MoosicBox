# Basic Audio Resampling Example

This example demonstrates the fundamental usage of the `moosicbox_resampler` package to convert audio between different sample rates.

## Summary

This example shows how to create a `Resampler` and use it to convert audio from one sample rate to another (44.1kHz to 48kHz in this case). It demonstrates the complete resampling workflow including buffering, chunk processing, and flushing remaining samples.

## What This Example Demonstrates

- Creating a `Resampler` with specified input and output sample rates
- Configuring signal specifications for stereo audio
- Processing audio in fixed-size chunks
- Handling the buffering behavior of the resampler
- Using the `flush()` method to process remaining samples at stream end
- Generating test audio data (sine wave) for demonstration
- Verifying the resampled output maintains correct duration

## Prerequisites

- Basic understanding of audio concepts (sample rate, channels, frames)
- Familiarity with Rust and the Symphonia audio library
- Understanding of digital signal processing concepts (helpful but not required)

## Running the Example

```bash
cargo run --manifest-path packages/resampler/examples/basic_usage/Cargo.toml
```

No additional arguments are needed - the example generates its own test audio.

## Expected Output

```
=== MoosicBox Resampler Example ===

Configuration:
  Input sample rate:  44100 Hz
  Output sample rate: 48000 Hz
  Channels:           2
  Chunk size:         2048 frames

Created resampler: 44kHz → 48kHz

Generating test audio (440 Hz sine wave)...
  Duration: 2.0 seconds
  Total input frames: 88200

Resampling audio...
....

Results:
  Input frames:            88200
  Input duration:          2.000 seconds
  Output frames:           96000
  Output duration:         2.000 seconds
  Expected output frames:  96000
  Difference:              0 frames

✓ Success! Audio duration preserved through resampling.

Resampling complete!
```

The output shows:

- The configuration parameters for the resampler
- Progress dots during processing (one per 0.5 seconds of input audio)
- Statistics comparing input and output frame counts
- Verification that the audio duration was preserved

## Code Walkthrough

### 1. Creating the Resampler

The core of the example is creating a `Resampler` instance:

```rust
// Create a signal specification for stereo 44.1kHz audio
let spec = SignalSpec::new(input_sample_rate, Layout::Stereo.into_channels());

// Create a resampler that converts from 44.1kHz to 48kHz
let mut resampler: Resampler<f32> = Resampler::new(
    spec,
    output_sample_rate, // 48000
    chunk_size,         // 2048 frames
);
```

Key parameters:

- `spec`: Input audio format (sample rate and channel layout)
- `output_sample_rate`: Target sample rate (48kHz)
- `chunk_size`: Number of frames processed per call (2048)

### 2. Generating Test Audio

The example generates a 440 Hz sine wave (musical note A4):

```rust
fn generate_sine_wave(
    spec: SignalSpec,
    duration: u64,
    offset: u64,
    sample_rate: f64,
) -> AudioBuffer<f32> {
    let mut buffer: AudioBuffer<f32> = AudioBuffer::new(duration, spec);
    buffer.render_reserved(Some(duration as usize));

    // Generate a 440 Hz sine wave at 50% amplitude
    let frequency = 440.0;
    let amplitude = 0.5;

    let (left, right) = buffer.chan_pair_mut(0, 1);
    for (i, (left_sample, right_sample)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        let frame_index = offset + i as u64;
        let t = frame_index as f64 / sample_rate;
        let value = (amplitude * (2.0 * PI * frequency * t).sin()) as f32;

        *left_sample = value;
        *right_sample = value;
    }

    buffer
}
```

This creates realistic audio data for testing the resampler.

### 3. Processing Audio Chunks

The main processing loop feeds audio to the resampler in chunks:

```rust
while frames_processed < total_input_frames {
    // Calculate current chunk size
    let remaining_frames = total_input_frames - frames_processed;
    let current_chunk_size = remaining_frames.min(chunk_size);

    // Generate input buffer
    let input_buffer = generate_sine_wave(
        spec,
        current_chunk_size,
        frames_processed,
        input_sample_rate as f64,
    );

    // Resample the chunk
    if let Some(output_samples) = resampler.resample(&input_buffer) {
        total_output_samples += output_samples.len();
    }

    frames_processed += current_chunk_size;
}
```

Important notes:

- `resample()` returns `None` until enough samples are buffered (at least `chunk_size`)
- Output is interleaved samples (e.g., `[L, R, L, R, ...]` for stereo)
- The resampler buffers input internally for fixed-size processing

### 4. Flushing Remaining Samples

At the end of the stream, flush any buffered samples:

```rust
if let Some(final_samples) = resampler.flush() {
    total_output_samples += final_samples.len();
}
```

The `flush()` method pads the internal buffer with silence and processes remaining samples, ensuring no audio is lost.

### 5. Verifying Results

The example calculates expected vs actual output:

```rust
let output_frames = total_output_samples / num_channels;
let expected_output_frames = (total_input_frames as f64
    * output_sample_rate as f64
    / input_sample_rate as f64) as usize;

let output_duration = output_frames as f64 / output_sample_rate as f64;
```

This verifies the sample rate conversion ratio is correct.

## Key Concepts

### Sample Rate Conversion

Sample rate conversion changes the number of samples per second while preserving audio content:

- **Upsampling** (44.1kHz → 48kHz): Creates more samples, increases temporal resolution
- **Downsampling** (48kHz → 44.1kHz): Reduces samples, decreases file size
- **Ratio**: Output frames = Input frames × (Output rate / Input rate)
    - Example: 88,200 frames × (48,000 / 44,100) = 96,000 frames

### FFT-Based Resampling

The resampler uses FFT (Fast Fourier Transform) based resampling via the `rubato` library:

- **High quality**: Minimal artifacts and distortion
- **Fixed input size**: Processes chunks of fixed size for predictable performance
- **Buffering**: Accumulates input until a full chunk is available

### Frame vs Sample

Understanding the terminology:

- **Frame**: One sample for all channels (e.g., stereo frame = left + right sample)
- **Sample**: A single audio value for one channel
- **Relationship**: Samples = Frames × Channels

For stereo audio at 44.1kHz:

- Sample rate: 44,100 frames/second
- Total samples: 88,200 samples/second (44,100 × 2 channels)

### Planar vs Interleaved

Audio data formats:

- **Planar** (input): Channels stored separately: `[LLLL...][RRRR...]`
- **Interleaved** (output): Channels alternated: `[L, R, L, R, ...]`

The resampler accepts planar `AudioBuffer` input and produces interleaved output.

### Chunk Size Selection

The `chunk_size` parameter affects:

- **Latency**: Larger chunks = more latency (must wait for full chunk)
- **Efficiency**: Larger chunks = fewer function calls, better performance
- **Memory**: Larger chunks = more memory for buffers

Common values:

- 512 - Very low latency (real-time audio)
- 2048 - Balanced (used in this example)
- 4096+ - High efficiency, acceptable latency

## Testing the Example

1. **Run with default settings**:

    ```bash
    cargo run --manifest-path packages/resampler/examples/basic_usage/Cargo.toml
    ```

2. **Modify parameters** in `main.rs` to experiment:
    - Change sample rates (e.g., 48000 → 44100 for downsampling)
    - Adjust chunk size (e.g., 512, 1024, 4096)
    - Modify test duration
    - Try mono audio (change to `Layout::Mono`)

3. **Verify correctness**:
    - Check that output duration matches input duration
    - Verify frame count ratio matches sample rate ratio
    - Ensure no panics or errors occur

## Troubleshooting

### Compilation errors

- Ensure workspace dependencies are up to date: `cargo update`
- Verify the `moosicbox_resampler` and `symphonia` versions match workspace

### Output frame count mismatch

- Small differences (±1 frame) are normal due to rounding
- Large differences indicate a bug in the test code or resampler configuration

### No output produced

- Verify that enough input samples are provided to fill at least one chunk
- Check that `flush()` is called to process remaining samples

### Panics in `generate_sine_wave()`

- Ensure the `SignalSpec` has exactly 2 channels (stereo)
- The code uses `chan_pair_mut(0, 1)` which requires stereo audio

## Related Examples

Other audio processing examples in the MoosicBox ecosystem:

- [`moosicbox_audio_decoder/examples/basic_usage`](../../../audio_decoder/examples/basic_usage/) - Decoding audio files to `AudioBuffer`
- [`moosicbox_audio_encoder/examples/basic_encoding`](../../../audio_encoder/examples/basic_encoding/) - Encoding audio data to various formats

### Using Resampler with Decoder

Combine the decoder and resampler for complete audio pipelines:

1. Decode audio file → `AudioBuffer<f32>`
2. Resample to target rate → interleaved samples
3. Send to audio output or encoder
