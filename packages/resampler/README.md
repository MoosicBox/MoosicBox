# MoosicBox Resampler

A lightweight audio resampling wrapper for the MoosicBox ecosystem, providing sample rate conversion using the rubato library. This package integrates with Symphonia audio buffers to enable seamless playback across different audio devices and formats.

## Features

- **Sample Rate Conversion**: Convert between different sample rates using FFT-based resampling
- **Symphonia Integration**: Works directly with Symphonia `AudioBuffer` types
- **Planar to Interleaved**: Converts planar audio input to interleaved output
- **Generic Sample Types**: Supports any Symphonia sample type that implements required traits
- **Fixed-Size Processing**: Uses fixed-size input chunks for predictable performance
- **Partial Buffer Flushing**: Handles remaining samples at end of stream

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_resampler = "0.1.4"
```

## Usage

### Basic Sample Rate Conversion

```rust
use moosicbox_resampler::Resampler;
use symphonia::core::audio::{AudioBuffer, SignalSpec, Channels};

// Configure signal spec for stereo input
let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

// Create resampler: 44.1kHz -> 48kHz
// duration = number of input samples per processing chunk
let mut resampler: Resampler<f32> = Resampler::new(
    spec,
    48000,  // target sample rate
    2048    // chunk size in frames
);

// Process audio buffer (planar input)
let input: AudioBuffer<f32> = /* ... */;

// Resample to interleaved output
if let Some(output_samples) = resampler.resample(&input) {
    // output_samples is &[f32] with interleaved channels
    println!("Resampled {} samples", output_samples.len());
}

// Flush remaining samples at end of stream
if let Some(final_samples) = resampler.flush() {
    println!("Flushed {} samples", final_samples.len());
}
```

### Converting to AudioBuffer Output

```rust
use moosicbox_resampler::Resampler;
use symphonia::core::audio::AudioBuffer;

let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 2048);

let input: AudioBuffer<f32> = /* ... */;

// Resample and get output as AudioBuffer
if let Some(output_buffer) = resampler.resample_to_audio_buffer(&input) {
    // output_buffer is AudioBuffer<f32>
    println!("Output buffer duration: {}", output_buffer.capacity());
}
```

### Standalone Conversion Function

```rust
use moosicbox_resampler::to_audio_buffer;
use symphonia::core::audio::{SignalSpec, Channels};

// Convert interleaved samples to AudioBuffer
let interleaved_samples: &[f32] = &[/* ... */];
let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

let audio_buffer = to_audio_buffer(interleaved_samples, spec);
```

## Programming Interface

### Core Types

```rust
pub struct Resampler<T> {
    // Internal rubato resampler (FftFixedIn)
    // Input/output buffers
    pub spec: SignalSpec,
}

impl<T> Resampler<T>
where
    T: Sample + ReversibleSample<f32>,
{
    /// Create a new resampler
    ///
    /// # Arguments
    /// * `spec` - Input signal specification (sample rate, channels, etc.)
    /// * `to_sample_rate` - Target output sample rate
    /// * `duration` - Chunk size in frames for fixed-size processing
    pub fn new(spec: SignalSpec, to_sample_rate: usize, duration: u64) -> Self;

    /// Resample a planar audio buffer to interleaved output
    ///
    /// Returns `None` if insufficient samples are buffered (< duration)
    pub fn resample(&mut self, input: &AudioBuffer<f32>) -> Option<&[T]>;

    /// Resample and return output as AudioBuffer
    pub fn resample_to_audio_buffer(&mut self, input: &AudioBuffer<f32>) -> Option<AudioBuffer<T>>;

    /// Flush remaining buffered samples
    ///
    /// Pads partial buffers with silence to process remaining samples
    pub fn flush(&mut self) -> Option<&[T]>;
}

/// Convert interleaved samples to Symphonia AudioBuffer
///
/// Note: Currently only supports stereo (2-channel) audio
pub fn to_audio_buffer<S>(samples: &[S], spec: SignalSpec) -> AudioBuffer<S>
where
    S: Sample;
```

## Implementation Details

### Resampling Engine

This package uses the [rubato](https://crates.io/crates/rubato) library's `FftFixedIn` resampler, which provides:

- FFT-based sample rate conversion
- Fixed input chunk size for predictable latency
- High-quality output suitable for audio playback

### Processing Model

The resampler operates on fixed-size chunks:

1. Input samples are buffered until `duration` frames are available
2. The buffered chunk is resampled using rubato
3. Output is converted from planar to interleaved format
4. Consumed input samples are removed from the buffer

### Channel Layout

- Input: Planar (non-interleaved) via Symphonia `AudioBuffer`
- Output: Interleaved samples
- `to_audio_buffer` currently supports stereo (2-channel) only

## Dependencies

- **rubato**: FFT-based resampler engine
- **symphonia**: Audio buffer types and sample conversion traits
- **arrayvec**: Stack-allocated arrays for channel references
- **log**: Logging support

## Testing

```bash
# Run all tests
cargo test

# Run tests with warnings
cargo test --features fail-on-warnings
```

## See Also

- [`moosicbox_audio_decoder`](../audio_decoder/README.md) - Audio decoding functionality
- [`moosicbox_audio_encoder`](../audio_encoder/README.md) - Audio encoding functionality
- [`moosicbox_audio_output`](../audio_output/README.md) - Audio output management
- [`moosicbox_player`](../player/README.md) - Audio playback engine
