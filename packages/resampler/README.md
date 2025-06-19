# MoosicBox Resampler

A high-performance audio resampling library for the MoosicBox ecosystem, providing sample rate conversion, format transformation, and audio quality optimization for seamless playback across different audio devices and formats.

## Features

- **Sample Rate Conversion**: Convert between different sample rates (44.1kHz, 48kHz, 96kHz, etc.)
- **High-Quality Algorithms**: Multiple resampling algorithms with configurable quality settings
- **Real-Time Processing**: Low-latency resampling for live audio streams
- **Format Support**: Handle various PCM formats and bit depths
- **Channel Mapping**: Support for mono, stereo, and multi-channel audio
- **Batch Processing**: Efficient batch resampling for large audio files
- **Memory Efficient**: Optimized memory usage for streaming applications
- **Quality Presets**: Predefined quality settings for different use cases
- **Async Support**: Non-blocking resampling operations with Tokio
- **SIMD Optimization**: Hardware-accelerated processing where available

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_resampler = "0.1.1"
```

## Usage

### Basic Sample Rate Conversion

```rust
use moosicbox_resampler::{Resampler, ResamplerConfig, SampleFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure resampler
    let config = ResamplerConfig {
        input_sample_rate: 44100,
        output_sample_rate: 48000,
        channels: 2,
        input_format: SampleFormat::F32,
        output_format: SampleFormat::F32,
        quality: ResamplerQuality::High,
    };

    // Create resampler instance
    let mut resampler = Resampler::new(config)?;

    // Input audio data (44.1kHz stereo)
    let input_samples: Vec<f32> = vec![0.0; 44100 * 2]; // 1 second of silence

    // Resample to 48kHz
    let output_samples = resampler.process(&input_samples)?;

    println!("Resampled {} input samples to {} output samples",
             input_samples.len(), output_samples.len());

    Ok(())
}
```

### Streaming Resampler

```rust
use moosicbox_resampler::{StreamingResampler, ResamplerBuffer};

async fn stream_resampling() -> Result<(), Box<dyn std::error::Error>> {
    let config = ResamplerConfig {
        input_sample_rate: 44100,
        output_sample_rate: 48000,
        channels: 2,
        input_format: SampleFormat::I16,
        output_format: SampleFormat::F32,
        quality: ResamplerQuality::Medium,
    };

    let mut streaming_resampler = StreamingResampler::new(config)?;

    // Simulate streaming audio chunks
    let chunk_size = 1024;
    let mut input_buffer = vec![0i16; chunk_size * 2]; // stereo

    loop {
        // Read audio chunk (e.g., from file or network)
        // fill_audio_chunk(&mut input_buffer).await?;

        // Process chunk
        let output_chunk = streaming_resampler.process_chunk(&input_buffer)?;

        if !output_chunk.is_empty() {
            // Send resampled audio to output
            // send_to_audio_output(&output_chunk).await?;
            println!("Processed chunk: {} -> {} samples",
                     input_buffer.len(), output_chunk.len());
        }

        // Break condition for example
        break;
    }

    // Flush remaining samples
    let final_samples = streaming_resampler.flush()?;
    if !final_samples.is_empty() {
        println!("Final flush: {} samples", final_samples.len());
    }

    Ok(())
}
```

### Multi-Channel Audio Processing

```rust
use moosicbox_resampler::{MultiChannelResampler, ChannelLayout};

async fn multi_channel_resampling() -> Result<(), Box<dyn std::error::Error>> {
    let config = ResamplerConfig {
        input_sample_rate: 96000,
        output_sample_rate: 44100,
        channels: 6, // 5.1 surround
        input_format: SampleFormat::F32,
        output_format: SampleFormat::F32,
        quality: ResamplerQuality::VeryHigh,
    };

    let mut resampler = MultiChannelResampler::new(config)?;

    // Configure channel layout
    resampler.set_channel_layout(ChannelLayout::Surround5_1)?;

    // Input: 96kHz 5.1 surround audio
    let input_samples: Vec<f32> = vec![0.0; 96000 * 6]; // 1 second

    // Downsample to 44.1kHz
    let output_samples = resampler.process(&input_samples)?;

    println!("Downsampled 5.1 audio: {} -> {} samples",
             input_samples.len(), output_samples.len());

    Ok(())
}
```

### Quality Comparison

```rust
use moosicbox_resampler::{ResamplerQuality, QualityAnalyzer};

async fn quality_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let base_config = ResamplerConfig {
        input_sample_rate: 44100,
        output_sample_rate: 22050, // Downsample by 2x
        channels: 2,
        input_format: SampleFormat::F32,
        output_format: SampleFormat::F32,
        quality: ResamplerQuality::Low, // Will be overridden
    };

    let qualities = vec![
        ResamplerQuality::Low,
        ResamplerQuality::Medium,
        ResamplerQuality::High,
        ResamplerQuality::VeryHigh,
    ];

    // Test signal: 1kHz sine wave
    let input_samples = generate_sine_wave(1000.0, 44100, 1.0);

    for quality in qualities {
        let mut config = base_config.clone();
        config.quality = quality;

        let mut resampler = Resampler::new(config)?;
        let start_time = std::time::Instant::now();

        let output_samples = resampler.process(&input_samples)?;

        let processing_time = start_time.elapsed();

        // Analyze quality metrics
        let analyzer = QualityAnalyzer::new();
        let metrics = analyzer.analyze(&input_samples, &output_samples, &config)?;

        println!("{:?} Quality:", quality);
        println!("  Processing time: {:.2}ms", processing_time.as_secs_f64() * 1000.0);
        println!("  SNR: {:.2} dB", metrics.signal_to_noise_ratio);
        println!("  THD: {:.4}%", metrics.total_harmonic_distortion * 100.0);
        println!("  Frequency response: {:.2} dB", metrics.frequency_response_flatness);
        println!();
    }

    Ok(())
}

fn generate_sine_wave(frequency: f64, sample_rate: u32, duration: f64) -> Vec<f32> {
    let samples = (sample_rate as f64 * duration) as usize;
    let mut wave = Vec::with_capacity(samples * 2); // stereo

    for i in 0..samples {
        let t = i as f64 / sample_rate as f64;
        let sample = (2.0 * std::f64::consts::PI * frequency * t).sin() as f32;
        wave.push(sample); // Left channel
        wave.push(sample); // Right channel
    }

    wave
}
```

### Advanced Configuration

```rust
use moosicbox_resampler::{ResamplerBuilder, WindowFunction, FilterType};

async fn advanced_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let resampler = ResamplerBuilder::new()
        .input_sample_rate(44100)
        .output_sample_rate(48000)
        .channels(2)
        .quality(ResamplerQuality::Custom {
            filter_length: 256,
            window_function: WindowFunction::Kaiser { beta: 8.5 },
            filter_type: FilterType::Sinc,
            cutoff_frequency: 0.95,
            transition_bandwidth: 0.05,
        })
        .buffer_size(4096)
        .enable_simd(true)
        .build()?;

    // Process audio with custom settings
    let input_samples: Vec<f32> = vec![0.0; 44100 * 2];
    let output_samples = resampler.process(&input_samples)?;

    Ok(())
}
```

## Programming Interface

### Core Types

```rust
pub struct Resampler {
    inner: Box<dyn ResamplerEngine>,
    config: ResamplerConfig,
    buffer: ResamplerBuffer,
}

impl Resampler {
    pub fn new(config: ResamplerConfig) -> Result<Self, ResamplerError>;
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>, ResamplerError>;
    pub fn process_interleaved(&mut self, input: &[f32], output: &mut [f32]) -> Result<usize, ResamplerError>;
    pub fn flush(&mut self) -> Result<Vec<f32>, ResamplerError>;
    pub fn reset(&mut self) -> Result<(), ResamplerError>;
    pub fn get_latency(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct ResamplerConfig {
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: u32,
    pub input_format: SampleFormat,
    pub output_format: SampleFormat,
    pub quality: ResamplerQuality,
}

#[derive(Debug, Clone)]
pub enum SampleFormat {
    I16,
    I24,
    I32,
    F32,
    F64,
}

#[derive(Debug, Clone)]
pub enum ResamplerQuality {
    Low,
    Medium,
    High,
    VeryHigh,
    Custom {
        filter_length: usize,
        window_function: WindowFunction,
        filter_type: FilterType,
        cutoff_frequency: f64,
        transition_bandwidth: f64,
    },
}
```

### Streaming Interface

```rust
pub struct StreamingResampler {
    resampler: Resampler,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
    buffer_size: usize,
}

impl StreamingResampler {
    pub fn new(config: ResamplerConfig) -> Result<Self, ResamplerError>;
    pub fn process_chunk(&mut self, input: &[f32]) -> Result<Vec<f32>, ResamplerError>;
    pub fn flush(&mut self) -> Result<Vec<f32>, ResamplerError>;
    pub fn set_buffer_size(&mut self, size: usize);
    pub fn get_output_latency(&self) -> Duration;
}

pub trait ResamplerEngine: Send + Sync {
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(usize, usize), ResamplerError>;
    fn flush(&mut self, output: &mut [f32]) -> Result<usize, ResamplerError>;
    fn reset(&mut self) -> Result<(), ResamplerError>;
    fn get_latency(&self) -> usize;
}
```

### Quality Analysis

```rust
pub struct QualityAnalyzer {
    fft_size: usize,
    window: Vec<f32>,
}

impl QualityAnalyzer {
    pub fn new() -> Self;
    pub fn analyze(&self, input: &[f32], output: &[f32], config: &ResamplerConfig) -> Result<QualityMetrics, ResamplerError>;
    pub fn measure_snr(&self, reference: &[f32], processed: &[f32]) -> f64;
    pub fn measure_thd(&self, signal: &[f32], fundamental_freq: f64, sample_rate: u32) -> f64;
}

#[derive(Debug)]
pub struct QualityMetrics {
    pub signal_to_noise_ratio: f64,
    pub total_harmonic_distortion: f64,
    pub frequency_response_flatness: f64,
    pub aliasing_level: f64,
    pub processing_latency: Duration,
}
```

## Configuration

### Environment Variables

- `RESAMPLER_DEFAULT_QUALITY`: Default quality setting (low, medium, high, very_high)
- `RESAMPLER_BUFFER_SIZE`: Default buffer size for streaming (default: 4096)
- `RESAMPLER_ENABLE_SIMD`: Enable SIMD optimizations (default: true)
- `RESAMPLER_THREAD_COUNT`: Number of threads for parallel processing (default: auto)

### Performance Tuning

```rust
use moosicbox_resampler::{PerformanceConfig, SIMDInstructions};

let performance_config = PerformanceConfig {
    enable_simd: true,
    simd_instructions: SIMDInstructions::Auto, // or AVX2, SSE4_1, etc.
    thread_count: Some(4),
    buffer_size: 8192,
    prefetch_size: 2048,
    memory_pool_size: 1024 * 1024, // 1MB
};

let resampler = ResamplerBuilder::new()
    .input_sample_rate(44100)
    .output_sample_rate(48000)
    .channels(2)
    .performance(performance_config)
    .build()?;
```

## Integration Examples

### Audio Player Integration

```rust
use moosicbox_resampler::{Resampler, ResamplerConfig};
use moosicbox_audio_output::AudioOutput;

struct AudioPlayer {
    resampler: Option<Resampler>,
    audio_output: AudioOutput,
    target_sample_rate: u32,
}

impl AudioPlayer {
    pub fn new(target_sample_rate: u32) -> Self {
        Self {
            resampler: None,
            audio_output: AudioOutput::new(),
            target_sample_rate,
        }
    }

    pub fn play_track(&mut self, track_data: &[f32], source_sample_rate: u32, channels: u32) -> Result<(), Box<dyn std::error::Error>> {
        let samples = if source_sample_rate != self.target_sample_rate {
            // Need resampling
            if self.resampler.is_none() ||
               self.resampler.as_ref().unwrap().config.input_sample_rate != source_sample_rate {
                let config = ResamplerConfig {
                    input_sample_rate: source_sample_rate,
                    output_sample_rate: self.target_sample_rate,
                    channels,
                    input_format: SampleFormat::F32,
                    output_format: SampleFormat::F32,
                    quality: ResamplerQuality::High,
                };
                self.resampler = Some(Resampler::new(config)?);
            }

            self.resampler.as_mut().unwrap().process(track_data)?
        } else {
            track_data.to_vec()
        };

        self.audio_output.play(&samples)?;
        Ok(())
    }
}
```

### File Format Conversion

```rust
use moosicbox_resampler::{Resampler, ResamplerConfig, SampleFormat};

async fn convert_audio_file(
    input_path: &str,
    output_path: &str,
    target_sample_rate: u32
) -> Result<(), Box<dyn std::error::Error>> {
    // Read input file (simplified)
    let (input_samples, source_sample_rate, channels) = read_audio_file(input_path)?;

    if source_sample_rate != target_sample_rate {
        let config = ResamplerConfig {
            input_sample_rate: source_sample_rate,
            output_sample_rate: target_sample_rate,
            channels,
            input_format: SampleFormat::F32,
            output_format: SampleFormat::F32,
            quality: ResamplerQuality::VeryHigh, // High quality for file conversion
        };

        let mut resampler = Resampler::new(config)?;
        let output_samples = resampler.process(&input_samples)?;

        // Write output file
        write_audio_file(output_path, &output_samples, target_sample_rate, channels)?;

        println!("Converted {} to {} ({}Hz -> {}Hz)",
                 input_path, output_path, source_sample_rate, target_sample_rate);
    } else {
        println!("No resampling needed for {}", input_path);
    }

    Ok(())
}
```

## Error Handling

```rust
use moosicbox_resampler::ResamplerError;

match resampler.process(&input_samples) {
    Ok(output) => {
        println!("Resampling successful: {} samples", output.len());
    }
    Err(ResamplerError::InvalidSampleRate { input, output }) => {
        eprintln!("Invalid sample rate conversion: {}Hz -> {}Hz", input, output);
    }
    Err(ResamplerError::UnsupportedFormat(format)) => {
        eprintln!("Unsupported sample format: {:?}", format);
    }
    Err(ResamplerError::BufferSizeMismatch { expected, actual }) => {
        eprintln!("Buffer size mismatch: expected {}, got {}", expected, actual);
    }
    Err(ResamplerError::InsufficientData) => {
        eprintln!("Insufficient input data for resampling");
    }
    Err(e) => eprintln!("Resampler error: {}", e),
}
```

## Performance Benchmarks

```bash
# Run performance benchmarks
cargo bench

# Test different quality settings
cargo bench --bench quality_comparison

# Test with different sample rates
cargo bench --bench sample_rate_conversion

# Memory usage profiling
cargo bench --bench memory_usage
```

## Testing

```bash
# Run all tests
cargo test

# Test with specific sample rates
cargo test test_44100_to_48000
cargo test test_96000_to_44100

# Quality tests
cargo test quality_tests

# Performance tests
cargo test --release performance_tests -- --ignored
```

## See Also

- [`moosicbox_audio_decoder`](../audio_decoder/README.md) - Audio decoding functionality
- [`moosicbox_audio_encoder`](../audio_encoder/README.md) - Audio encoding functionality
- [`moosicbox_audio_output`](../audio_output/README.md) - Audio output management
- [`moosicbox_player`](../player/README.md) - Audio playback engine
