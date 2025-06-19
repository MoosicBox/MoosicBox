# MoosicBox Audio Decoder

High-performance audio decoding library supporting multiple audio formats for the MoosicBox ecosystem.

## Overview

The MoosicBox Audio Decoder package provides:

- **Multi-Format Support**: Decode MP3, FLAC, AAC, Opus, Vorbis, and more
- **High Performance**: Optimized decoding with minimal memory footprint
- **Streaming Support**: Decode audio streams without loading entire files
- **Metadata Extraction**: Extract audio metadata during decoding
- **Error Recovery**: Robust error handling and stream recovery
- **Cross-Platform**: Works on Linux, macOS, Windows, and embedded systems
- **Low Latency**: Minimal buffering for real-time applications
- **Thread Safety**: Safe for concurrent use across multiple threads

## Features

### Supported Formats
- **MP3**: MPEG-1/2/2.5 Layer III with all bitrates and sample rates
- **FLAC**: Free Lossless Audio Codec with full specification support
- **AAC**: Advanced Audio Coding (LC, HE-AAC, HE-AACv2)
- **Opus**: Modern codec optimized for internet streaming
- **Vorbis**: Ogg Vorbis with full quality range support
- **WAV**: Uncompressed PCM and compressed formats
- **AIFF**: Audio Interchange File Format
- **M4A**: MPEG-4 Audio container format
- **WMA**: Windows Media Audio (basic support)

### Decoding Capabilities
- **Streaming Decoding**: Process audio without loading entire files
- **Seeking Support**: Fast seeking to arbitrary positions
- **Gapless Playback**: Seamless transitions between tracks
- **Sample Rate Conversion**: On-the-fly resampling
- **Channel Mixing**: Convert between mono, stereo, and multi-channel
- **Bit Depth Conversion**: Support for 16-bit, 24-bit, and 32-bit output
- **Metadata Preservation**: Extract and preserve audio metadata

### Performance Features
- **Zero-Copy Decoding**: Minimize memory allocations
- **Vectorized Operations**: SIMD optimizations where available
- **Adaptive Buffering**: Dynamic buffer sizing based on content
- **Multi-Threading**: Parallel decoding for supported formats
- **Memory Efficient**: Configurable memory usage limits
- **Cache Friendly**: Optimized memory access patterns

## Installation

### From Source

```bash
# Install system dependencies
sudo apt update
sudo apt install build-essential pkg-config

# Audio format dependencies
sudo apt install libflac-dev libvorbis-dev libopus-dev
sudo apt install libmp3lame-dev libfaad-dev

# Clone and build
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --release --package moosicbox_audio_decoder
```

### Cargo Dependencies

```toml
[dependencies]
moosicbox_audio_decoder = { path = "../audio_decoder" }

# Optional: Enable specific format support
moosicbox_audio_decoder = {
    path = "../audio_decoder",
    features = ["mp3", "flac", "aac", "opus", "vorbis"]
}
```

## Usage

### Basic Decoding

```rust
use moosicbox_audio_decoder::{AudioDecoder, DecoderConfig, AudioFormat};
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open audio file
    let file = File::open("music.flac")?;

    // Create decoder with default configuration
    let mut decoder = AudioDecoder::new(file)?;

    // Get audio information
    let info = decoder.info();
    println!("Format: {:?}", info.format);
    println!("Sample rate: {} Hz", info.sample_rate);
    println!("Channels: {}", info.channels);
    println!("Duration: {:.2} seconds", info.duration_seconds());

    // Decode audio samples
    let mut buffer = vec![0f32; 4096];
    while let Ok(samples_read) = decoder.read_samples(&mut buffer) {
        if samples_read == 0 {
            break; // End of stream
        }

        // Process decoded samples
        process_audio_samples(&buffer[..samples_read]);
    }

    Ok(())
}

fn process_audio_samples(samples: &[f32]) {
    // Your audio processing code here
    println!("Processed {} samples", samples.len());
}
```

### Advanced Configuration

```rust
use moosicbox_audio_decoder::{
    AudioDecoder, DecoderConfig, OutputFormat, ResamplingQuality
};

async fn decode_with_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = DecoderConfig {
        // Output format configuration
        output_format: OutputFormat {
            sample_rate: Some(44100),    // Resample to 44.1kHz
            channels: Some(2),           // Convert to stereo
            bit_depth: Some(16),         // 16-bit output
            sample_format: SampleFormat::S16LE,
        },

        // Performance settings
        buffer_size: 8192,
        max_memory_usage: 64 * 1024 * 1024, // 64MB limit
        enable_simd: true,
        threading: ThreadingMode::Auto,

        // Quality settings
        resampling_quality: ResamplingQuality::High,
        dithering: true,

        // Error handling
        error_recovery: true,
        strict_mode: false,
    };

    let file = File::open("music.mp3")?;
    let mut decoder = AudioDecoder::with_config(file, config)?;

    // Decode with configured settings
    let mut output_buffer = vec![0i16; 4096];
    while let Ok(samples) = decoder.read_samples_i16(&mut output_buffer) {
        if samples == 0 { break; }

        // Process 16-bit samples
        process_i16_samples(&output_buffer[..samples]);
    }

    Ok(())
}
```

### Streaming Decoding

```rust
use moosicbox_audio_decoder::{StreamingDecoder, StreamSource};
use tokio::io::AsyncRead;

async fn decode_stream<R: AsyncRead + Unpin>(
    stream: R
) -> Result<(), Box<dyn std::error::Error>> {
    let source = StreamSource::new(stream);
    let mut decoder = StreamingDecoder::new(source).await?;

    // Process audio in chunks as it arrives
    let mut chunk_buffer = vec![0f32; 2048];

    loop {
        match decoder.read_chunk(&mut chunk_buffer).await {
            Ok(0) => break, // End of stream
            Ok(samples) => {
                // Process this chunk
                process_streaming_chunk(&chunk_buffer[..samples]);
            },
            Err(e) if e.is_recoverable() => {
                // Handle recoverable errors (network issues, etc.)
                eprintln!("Recoverable error: {}", e);
                continue;
            },
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

fn process_streaming_chunk(samples: &[f32]) {
    // Process audio chunk for real-time playback
    println!("Processing {} samples", samples.len());
}
```

### Seeking and Random Access

```rust
use moosicbox_audio_decoder::{AudioDecoder, SeekMode};
use std::time::Duration;

fn demonstrate_seeking() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("long_track.flac")?;
    let mut decoder = AudioDecoder::new(file)?;

    // Seek to 2 minutes into the track
    let target_time = Duration::from_secs(120);
    decoder.seek(target_time, SeekMode::Accurate)?;

    // Read samples from the new position
    let mut buffer = vec![0f32; 4096];
    let samples_read = decoder.read_samples(&mut buffer)?;

    println!("Reading from {}s position: {} samples",
             target_time.as_secs(), samples_read);

    // Seek by sample offset
    let sample_offset = 44100 * 60; // 1 minute at 44.1kHz
    decoder.seek_samples(sample_offset, SeekMode::Fast)?;

    // Get current position
    let current_pos = decoder.current_position();
    println!("Current position: {:.2}s", current_pos.as_secs_f64());

    Ok(())
}
```

### Metadata Extraction

```rust
use moosicbox_audio_decoder::{AudioDecoder, MetadataExtractor};

fn extract_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("song.mp3")?;
    let decoder = AudioDecoder::new(file)?;

    // Get metadata from decoder
    let metadata = decoder.metadata();

    println!("Title: {}", metadata.title.unwrap_or("Unknown".to_string()));
    println!("Artist: {}", metadata.artist.unwrap_or("Unknown".to_string()));
    println!("Album: {}", metadata.album.unwrap_or("Unknown".to_string()));
    println!("Year: {}", metadata.year.unwrap_or(0));
    println!("Track: {}/{}",
             metadata.track_number.unwrap_or(0),
             metadata.total_tracks.unwrap_or(0));

    // Genre information
    if let Some(genre) = metadata.genre {
        println!("Genre: {}", genre);
    }

    // Technical information
    println!("Bitrate: {} kbps", metadata.bitrate.unwrap_or(0));
    println!("Encoding: {}", metadata.encoding_info.unwrap_or("Unknown".to_string()));

    // Album art
    if let Some(artwork) = metadata.artwork {
        println!("Album art: {} bytes, format: {:?}",
                 artwork.data.len(), artwork.format);

        // Save album art
        std::fs::write("album_art.jpg", artwork.data)?;
    }

    Ok(())
}
```

### Format-Specific Decoding

```rust
use moosicbox_audio_decoder::{
    FlacDecoder, Mp3Decoder, AacDecoder, OpusDecoder, VorbisDecoder
};

// FLAC-specific decoding with advanced options
fn decode_flac_advanced() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("audio.flac")?;
    let mut decoder = FlacDecoder::new(file)?;

    // FLAC-specific configuration
    decoder.set_md5_checking(true);
    decoder.set_metadata_callback(|metadata| {
        println!("FLAC metadata block: {:?}", metadata);
    });

    // Decode with FLAC-specific features
    let mut buffer = vec![0i32; 4096]; // FLAC can output 32-bit samples
    while let Ok(samples) = decoder.read_samples_i32(&mut buffer) {
        if samples == 0 { break; }
        process_high_resolution_samples(&buffer[..samples]);
    }

    Ok(())
}

// MP3-specific decoding with frame information
fn decode_mp3_with_frames() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("audio.mp3")?;
    let mut decoder = Mp3Decoder::new(file)?;

    // Get MP3-specific information
    let mp3_info = decoder.mp3_info();
    println!("MP3 version: {:?}", mp3_info.version);
    println!("Layer: {}", mp3_info.layer);
    println!("Bitrate mode: {:?}", mp3_info.bitrate_mode);

    // Decode frame by frame
    while let Ok(frame) = decoder.read_frame() {
        println!("Frame: {} samples, bitrate: {} kbps",
                 frame.samples.len(), frame.bitrate);

        process_mp3_frame(&frame);
    }

    Ok(())
}

fn process_high_resolution_samples(samples: &[i32]) {
    // Process high-resolution audio samples
}

fn process_mp3_frame(frame: &Mp3Frame) {
    // Process MP3 frame data
}
```

### Error Handling and Recovery

```rust
use moosicbox_audio_decoder::{AudioDecoder, DecoderError, ErrorRecovery};

fn robust_decoding() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("potentially_corrupted.mp3")?;
    let mut decoder = AudioDecoder::new(file)?;

    // Enable error recovery
    decoder.set_error_recovery(ErrorRecovery::Aggressive);

    let mut buffer = vec![0f32; 4096];
    let mut total_errors = 0;

    loop {
        match decoder.read_samples(&mut buffer) {
            Ok(0) => break, // End of stream
            Ok(samples) => {
                process_audio_samples(&buffer[..samples]);
            },
            Err(DecoderError::CorruptedData { position, recoverable }) => {
                total_errors += 1;
                eprintln!("Corrupted data at position {}", position);

                if recoverable {
                    // Try to recover and continue
                    if let Ok(()) = decoder.recover() {
                        continue;
                    }
                }

                // Skip to next valid frame
                if let Ok(()) = decoder.skip_to_next_frame() {
                    continue;
                }

                break; // Unrecoverable error
            },
            Err(DecoderError::UnsupportedFormat { format, reason }) => {
                eprintln!("Unsupported format {}: {}", format, reason);
                break;
            },
            Err(e) => return Err(e.into()),
        }
    }

    println!("Decoding completed with {} errors", total_errors);
    Ok(())
}
```

### Batch Processing

```rust
use moosicbox_audio_decoder::{BatchDecoder, BatchConfig, ProcessingMode};
use std::path::Path;

async fn batch_decode_directory<P: AsRef<Path>>(
    input_dir: P,
    output_dir: P,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = BatchConfig {
        processing_mode: ProcessingMode::Parallel,
        max_concurrent: 4,
        output_format: OutputFormat {
            sample_rate: Some(44100),
            channels: Some(2),
            bit_depth: Some(16),
            sample_format: SampleFormat::S16LE,
        },
        error_handling: ErrorHandling::SkipAndContinue,
    };

    let mut batch_decoder = BatchDecoder::new(config);

    // Add files to batch
    for entry in std::fs::read_dir(input_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map_or(false, |ext| {
            matches!(ext.to_str(), Some("mp3" | "flac" | "aac" | "opus"))
        }) {
            batch_decoder.add_file(path);
        }
    }

    // Process all files
    let results = batch_decoder.process_all(output_dir).await?;

    // Report results
    for result in results {
        match result {
            Ok(info) => {
                println!("✓ {} -> {} ({:.2}s)",
                         info.input_file.display(),
                         info.output_file.display(),
                         info.processing_time.as_secs_f64());
            },
            Err(e) => {
                eprintln!("✗ {}: {}", e.input_file.display(), e.error);
            }
        }
    }

    Ok(())
}
```

## Programming Interface

### Core Types

```rust
use moosicbox_audio_decoder::*;

// Audio information structure
pub struct AudioInfo {
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: Option<u8>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u32>,
    pub is_lossless: bool,
    pub is_variable_bitrate: bool,
}

// Decoder configuration
pub struct DecoderConfig {
    pub output_format: OutputFormat,
    pub buffer_size: usize,
    pub max_memory_usage: usize,
    pub enable_simd: bool,
    pub threading: ThreadingMode,
    pub resampling_quality: ResamplingQuality,
    pub error_recovery: bool,
}

// Sample formats
pub enum SampleFormat {
    F32LE,    // 32-bit float little-endian
    F32BE,    // 32-bit float big-endian
    S32LE,    // 32-bit signed integer little-endian
    S32BE,    // 32-bit signed integer big-endian
    S16LE,    // 16-bit signed integer little-endian
    S16BE,    // 16-bit signed integer big-endian
    U8,       // 8-bit unsigned integer
}

// Threading modes
pub enum ThreadingMode {
    Single,           // Single-threaded
    Multi(usize),     // Multi-threaded with specified thread count
    Auto,             // Automatic thread count based on CPU cores
}
```

### Trait Implementations

```rust
// Custom decoder implementation
use moosicbox_audio_decoder::{Decoder, DecoderResult};

pub struct CustomDecoder {
    // Your decoder state
}

impl Decoder for CustomDecoder {
    fn info(&self) -> &AudioInfo {
        &self.audio_info
    }

    fn read_samples(&mut self, buffer: &mut [f32]) -> DecoderResult<usize> {
        // Your decoding implementation
        Ok(samples_written)
    }

    fn seek(&mut self, position: Duration) -> DecoderResult<()> {
        // Your seeking implementation
        Ok(())
    }

    fn current_position(&self) -> Duration {
        // Return current playback position
        self.current_pos
    }
}

// Register custom decoder
AudioDecoder::register_decoder("custom", Box::new(CustomDecoderFactory));
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MOOSICBOX_DECODER_BUFFER_SIZE` | Default buffer size | `4096` |
| `MOOSICBOX_DECODER_MAX_MEMORY` | Maximum memory usage (bytes) | `67108864` |
| `MOOSICBOX_DECODER_SIMD` | Enable SIMD optimizations | `true` |
| `MOOSICBOX_DECODER_THREADS` | Number of decoder threads | `auto` |
| `MOOSICBOX_DECODER_QUALITY` | Default resampling quality | `high` |

### Feature Flags

```toml
[dependencies.moosicbox_audio_decoder]
path = "../audio_decoder"
default-features = false
features = [
    "mp3",           # MP3 support via minimp3
    "flac",          # FLAC support via libflac
    "aac",           # AAC support via faad2
    "opus",          # Opus support via libopus
    "vorbis",        # Vorbis support via libvorbis
    "wav",           # WAV support (built-in)
    "aiff",          # AIFF support (built-in)
    "m4a",           # M4A support
    "wma",           # WMA support (limited)
    "simd",          # SIMD optimizations
    "threading",     # Multi-threading support
    "resampling",    # High-quality resampling
    "metadata",      # Metadata extraction
    "streaming",     # Streaming decoder support
]
```

## Performance Optimization

### Memory Usage

```rust
use moosicbox_audio_decoder::{AudioDecoder, MemoryConfig};

// Configure memory usage
let memory_config = MemoryConfig {
    max_buffer_size: 32 * 1024,      // 32KB max buffer
    max_lookahead: 8 * 1024,         // 8KB lookahead
    enable_memory_mapping: true,      // Use mmap for large files
    preload_threshold: 1024 * 1024,  // Preload files < 1MB
};

let mut decoder = AudioDecoder::with_memory_config(file, memory_config)?;
```

### SIMD Optimizations

```rust
// Enable SIMD optimizations
use moosicbox_audio_decoder::{SimdConfig, SimdLevel};

let simd_config = SimdConfig {
    level: SimdLevel::Auto,  // Detect best SIMD level
    enable_avx2: true,       // Enable AVX2 if available
    enable_neon: true,       // Enable ARM NEON if available
    fallback_scalar: true,   // Fallback to scalar code
};

decoder.configure_simd(simd_config)?;
```

### Threading Configuration

```rust
use moosicbox_audio_decoder::{ThreadConfig, ThreadPriority};

let thread_config = ThreadConfig {
    decoder_threads: 2,
    io_threads: 1,
    priority: ThreadPriority::High,
    affinity: Some(vec![0, 1]), // Pin to specific CPU cores
};

decoder.configure_threading(thread_config)?;
```

## Troubleshooting

### Common Issues

1. **Unsupported format errors**
   ```bash
   # Check available decoders
   cargo build --features "mp3,flac,aac,opus,vorbis"

   # Verify file format
   file audio_file.mp3
   ```

2. **Memory usage too high**
   ```rust
   // Reduce buffer sizes
   let config = DecoderConfig {
       buffer_size: 1024,
       max_memory_usage: 16 * 1024 * 1024, // 16MB limit
       ..Default::default()
   };
   ```

3. **Poor performance**
   ```rust
   // Enable optimizations
   let config = DecoderConfig {
       enable_simd: true,
       threading: ThreadingMode::Auto,
       ..Default::default()
   };
   ```

4. **Seeking accuracy issues**
   ```rust
   // Use accurate seeking mode
   decoder.seek(position, SeekMode::Accurate)?;

   // Or enable frame-accurate seeking
   decoder.set_seek_mode(SeekMode::FrameAccurate);
   ```

## See Also

- [MoosicBox Audio Encoder](../audio_encoder/README.md) - Audio encoding counterpart
- [MoosicBox Player](../player/README.md) - Audio playback engine
- [MoosicBox Files](../files/README.md) - File handling and streaming
- [MoosicBox Resampler](../resampler/README.md) - Audio resampling utilities
