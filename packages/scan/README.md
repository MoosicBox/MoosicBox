# MoosicBox Scan

Intelligent music library scanning system for discovering, analyzing, and indexing audio files across multiple sources.

## Overview

The MoosicBox Scan package provides:

- **Multi-Format Support**: Scan MP3, FLAC, AAC, Opus, OGG, and more
- **Metadata Extraction**: Rich ID3, Vorbis, and other metadata parsing
- **Audio Analysis**: Automatic BPM detection, key detection, and audio fingerprinting
- **Smart Scanning**: Incremental scans and change detection
- **Performance Optimized**: Parallel scanning with configurable concurrency
- **Duplicate Detection**: Find and manage duplicate tracks across sources

## Features

### File Discovery
- **Recursive Scanning**: Deep directory tree traversal
- **File Type Detection**: Automatic audio format identification
- **Path Filtering**: Include/exclude patterns for selective scanning
- **Symlink Handling**: Configurable symlink following behavior
- **Hidden File Support**: Option to scan hidden files and directories

### Metadata Extraction
- **ID3 Tags**: Full support for ID3v1, ID3v2.3, and ID3v2.4
- **Vorbis Comments**: Support for FLAC, OGG, and Opus files
- **MP4 Tags**: iTunes-compatible metadata for AAC/M4A files
- **Custom Fields**: Extract custom metadata fields
- **Encoding Detection**: Automatic character encoding detection

### Audio Analysis
- **Duration Calculation**: Precise track duration detection
- **Bitrate Analysis**: Audio quality assessment
- **Sample Rate Detection**: Audio format specifications
- **BPM Detection**: Automatic tempo analysis
- **Key Detection**: Musical key identification
- **Audio Fingerprinting**: Unique track identification

## Usage

### Basic Scanning

```rust
use moosicbox_scan::{Scanner, ScanConfig, ScanOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure scanner
    let config = ScanConfig {
        paths: vec!["/home/user/Music".into()],
        recursive: true,
        follow_symlinks: false,
        include_hidden: false,
        max_depth: None,
        parallel_jobs: 4,
    };

    // Create scanner
    let scanner = Scanner::new(config).await?;

    // Perform scan
    let scan_result = scanner.scan().await?;

    println!("Scan completed:");
    println!("  Files found: {}", scan_result.files_scanned);
    println!("  Tracks added: {}", scan_result.tracks_added);
    println!("  Errors: {}", scan_result.errors.len());

    Ok(())
}
```

### Advanced Scanning Options

```rust
use moosicbox_scan::{Scanner, ScanOptions, FileFilter, MetadataOptions};

async fn advanced_scan() -> Result<(), Box<dyn std::error::Error>> {
    let scan_options = ScanOptions {
        // File filtering
        file_filter: FileFilter {
            include_extensions: vec!["mp3", "flac", "m4a", "ogg", "opus"],
            exclude_patterns: vec!["**/.*", "**/*temp*"],
            min_file_size: Some(1024), // 1KB minimum
            max_file_size: Some(500 * 1024 * 1024), // 500MB maximum
        },

        // Metadata extraction
        metadata_options: MetadataOptions {
            extract_artwork: true,
            extract_lyrics: false,
            calculate_replay_gain: true,
            detect_bpm: true,
            detect_key: false,
            generate_fingerprint: true,
            encoding_detection: true,
        },

        // Performance tuning
        parallel_jobs: 8,
        chunk_size: 100,
        memory_limit_mb: 512,

        // Incremental scanning
        incremental: true,
        compare_modified_time: true,
        compare_file_size: true,
        force_rescan: false,
    };

    let scanner = Scanner::with_options(config, scan_options).await?;
    let result = scanner.scan().await?;

    // Process results
    for track in result.tracks {
        println!("Found: {} - {} ({}:{:02})",
                 track.artist, track.title,
                 track.duration / 60, track.duration % 60);

        if let Some(bpm) = track.bpm {
            println!("  BPM: {}", bpm);
        }

        if let Some(fingerprint) = track.fingerprint {
            println!("  Fingerprint: {}", fingerprint);
        }
    }

    Ok(())
}
```

### Incremental Scanning

```rust
use moosicbox_scan::{Scanner, IncrementalScanOptions};

async fn incremental_scan() -> Result<(), Box<dyn std::error::Error>> {
    let config = ScanConfig::default();
    let scanner = Scanner::new(config).await?;

    // Perform initial full scan
    let initial_result = scanner.scan().await?;
    println!("Initial scan: {} tracks", initial_result.tracks_added);

    // Save scan state
    scanner.save_scan_state("./scan_state.json").await?;

    // Later, perform incremental scan
    let incremental_options = IncrementalScanOptions {
        state_file: "./scan_state.json".into(),
        check_modifications: true,
        check_deletions: true,
        update_existing: true,
    };

    let incremental_result = scanner.scan_incremental(incremental_options).await?;

    println!("Incremental scan:");
    println!("  New tracks: {}", incremental_result.tracks_added);
    println!("  Updated tracks: {}", incremental_result.tracks_updated);
    println!("  Removed tracks: {}", incremental_result.tracks_removed);

    Ok(())
}
```

### Real-Time Monitoring

```rust
use moosicbox_scan::{Scanner, WatcherConfig, FileEvent};
use tokio::sync::mpsc;

async fn setup_file_watching() -> Result<(), Box<dyn std::error::Error>> {
    let watcher_config = WatcherConfig {
        paths: vec!["/home/user/Music".into()],
        recursive: true,
        debounce_ms: 1000, // Wait 1 second for file operations to complete
        batch_size: 10,    // Process up to 10 events at once
    };

    let (tx, mut rx) = mpsc::channel(100);
    let scanner = Scanner::new(ScanConfig::default()).await?;

    // Start file watcher
    let _watcher = scanner.start_watcher(watcher_config, tx).await?;

    // Process file events
    while let Some(events) = rx.recv().await {
        for event in events {
            match event {
                FileEvent::Created(path) => {
                    println!("New file: {:?}", path);
                    let result = scanner.scan_file(&path).await?;
                    if let Some(track) = result.track {
                        println!("Added track: {} - {}", track.artist, track.title);
                    }
                },
                FileEvent::Modified(path) => {
                    println!("Modified file: {:?}", path);
                    scanner.rescan_file(&path).await?;
                },
                FileEvent::Deleted(path) => {
                    println!("Deleted file: {:?}", path);
                    scanner.remove_file(&path).await?;
                },
            }
        }
    }

    Ok(())
}
```

### Metadata Extraction

```rust
use moosicbox_scan::{MetadataExtractor, AudioMetadata, ImageData};

async fn extract_metadata(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let extractor = MetadataExtractor::new();

    // Extract basic metadata
    let metadata = extractor.extract_metadata(file_path).await?;

    println!("Metadata for: {}", file_path);
    println!("  Title: {}", metadata.title.unwrap_or_default());
    println!("  Artist: {}", metadata.artist.unwrap_or_default());
    println!("  Album: {}", metadata.album.unwrap_or_default());
    println!("  Year: {}", metadata.year.unwrap_or_default());
    println!("  Genre: {}", metadata.genre.unwrap_or_default());
    println!("  Duration: {}s", metadata.duration);
    println!("  Bitrate: {}kbps", metadata.bitrate);
    println!("  Sample Rate: {}Hz", metadata.sample_rate);

    // Extract artwork
    if let Some(artwork) = extractor.extract_artwork(file_path).await? {
        println!("  Artwork: {}x{} pixels ({})",
                 artwork.width, artwork.height, artwork.format);

        // Save artwork to file
        std::fs::write("artwork.jpg", artwork.data)?;
    }

    // Extract lyrics
    if let Some(lyrics) = extractor.extract_lyrics(file_path).await? {
        println!("  Lyrics: {} characters", lyrics.text.len());
        if lyrics.is_synchronized {
            println!("  Synchronized lyrics with {} timestamps", lyrics.timestamps.len());
        }
    }

    // Calculate audio fingerprint
    let fingerprint = extractor.calculate_fingerprint(file_path).await?;
    println!("  Fingerprint: {}", fingerprint);

    Ok(())
}
```

### Duplicate Detection

```rust
use moosicbox_scan::{DuplicateDetector, DuplicateOptions, DuplicateSet};

async fn find_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    let duplicate_options = DuplicateOptions {
        match_by_fingerprint: true,
        match_by_metadata: true,
        match_by_filename: false,
        fingerprint_threshold: 0.95,
        metadata_similarity_threshold: 0.9,
        group_by_album: true,
    };

    let detector = DuplicateDetector::new(duplicate_options);
    let duplicates = detector.find_duplicates("/home/user/Music").await?;

    for duplicate_set in duplicates {
        println!("Duplicate set (confidence: {:.2}):", duplicate_set.confidence);

        for track in duplicate_set.tracks {
            println!("  {} - {} ({}kbps, {})",
                     track.artist, track.title, track.bitrate, track.file_path);
        }

        // Suggest which file to keep
        let best_quality = duplicate_set.suggest_best_quality();
        println!("  Suggested to keep: {}", best_quality.file_path);

        println!();
    }

    Ok(())
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `SCAN_PARALLEL_JOBS` | Number of parallel scan jobs | `4` |
| `SCAN_CHUNK_SIZE` | Files to process per chunk | `100` |
| `SCAN_MEMORY_LIMIT` | Memory limit in MB | `512` |
| `SCAN_ENABLE_BPM` | Enable BPM detection | `false` |
| `SCAN_ENABLE_KEY` | Enable key detection | `false` |
| `SCAN_ENABLE_FINGERPRINT` | Enable audio fingerprinting | `true` |
| `SCAN_CACHE_DIR` | Cache directory | `./cache/scan` |

### File Type Support

```rust
use moosicbox_scan::{AudioFormat, SupportedFormats};

// Check supported formats
let formats = SupportedFormats::new();

println!("Supported formats:");
for format in formats.iter() {
    println!("  {}: {} ({})", format.extension, format.name,
             if format.lossless { "lossless" } else { "lossy" });
}

// Configure format-specific options
let format_config = AudioFormatConfig {
    mp3: Mp3Config {
        strict_parsing: false,
        encoding_detection: true,
        id3v1_fallback: true,
    },
    flac: FlacConfig {
        verify_checksum: true,
        extract_cuesheet: true,
    },
    m4a: M4aConfig {
        extract_chapters: false,
    },
};
```

### Performance Tuning

```rust
use moosicbox_scan::{ScanPerformanceConfig, MemoryConfig};

let performance_config = ScanPerformanceConfig {
    // Concurrency settings
    parallel_jobs: 8,           // Number of parallel workers
    chunk_size: 50,             // Files per processing chunk
    queue_size: 1000,           // Internal queue size

    // Memory management
    memory_config: MemoryConfig {
        max_memory_mb: 1024,     // Maximum memory usage
        cache_size_mb: 256,      // Metadata cache size
        image_cache_mb: 128,     // Artwork cache size
        cleanup_threshold: 0.8,  // Cleanup when 80% full
    },

    // I/O optimization
    read_buffer_size: 64 * 1024, // 64KB read buffer
    prefetch_count: 10,          // Files to prefetch
    use_memory_mapping: true,    // Use mmap for large files

    // Analysis settings
    skip_analysis_for_large_files: true,
    large_file_threshold_mb: 100,
    timeout_seconds: 30,         // Per-file timeout
};
```

## Feature Flags

- `scan` - Core scanning functionality
- `scan-metadata` - Metadata extraction capabilities
- `scan-artwork` - Album artwork extraction
- `scan-lyrics` - Lyrics extraction
- `scan-bpm` - BPM detection
- `scan-key` - Musical key detection
- `scan-fingerprint` - Audio fingerprinting
- `scan-watch` - Real-time file watching
- `scan-parallel` - Parallel processing support

## Integration with MoosicBox

### Library Integration

```toml
[dependencies]
moosicbox-scan = { path = "../scan", features = ["scan-metadata", "scan-fingerprint"] }
```

```rust
use moosicbox_scan::Scanner;
use moosicbox_library::LibraryManager;

async fn setup_library_scanning() -> Result<(), Box<dyn std::error::Error>> {
    let scanner = Scanner::new(config).await?;
    let library = LibraryManager::new(library_config).await?;

    // Connect scanner to library
    library.set_scanner(scanner).await?;

    // Perform initial scan
    library.scan_and_import("/home/user/Music").await?;

    Ok(())
}
```

### Server Integration

```rust
use moosicbox_scan::Scanner;
use moosicbox_server::Server;

async fn setup_server_scanning() -> Result<(), Box<dyn std::error::Error>> {
    let scanner = Scanner::new(config).await?;
    let mut server = Server::new().await?;

    // Add scanning endpoints
    server.add_scan_routes(scanner).await?;

    Ok(())
}
```

## Performance Optimization

### Scanning Strategies

```rust
// Fast scan (metadata only)
let fast_options = ScanOptions {
    metadata_options: MetadataOptions {
        extract_artwork: false,
        extract_lyrics: false,
        calculate_replay_gain: false,
        detect_bpm: false,
        detect_key: false,
        generate_fingerprint: false,
    },
    parallel_jobs: 16,
    ..Default::default()
};

// Deep scan (full analysis)
let deep_options = ScanOptions {
    metadata_options: MetadataOptions {
        extract_artwork: true,
        extract_lyrics: true,
        calculate_replay_gain: true,
        detect_bpm: true,
        detect_key: true,
        generate_fingerprint: true,
    },
    parallel_jobs: 4,
    ..Default::default()
};

// Balanced scan
let balanced_options = ScanOptions {
    metadata_options: MetadataOptions {
        extract_artwork: true,
        extract_lyrics: false,
        calculate_replay_gain: false,
        detect_bpm: true,
        detect_key: false,
        generate_fingerprint: true,
    },
    parallel_jobs: 8,
    ..Default::default()
};
```

### Memory Management

```rust
use moosicbox_scan::{MemoryMonitor, CacheManager};

// Monitor memory usage during scanning
let memory_monitor = MemoryMonitor::new();
let scanner = Scanner::with_memory_monitor(config, memory_monitor).await?;

// Configure intelligent caching
let cache_manager = CacheManager::new(CacheConfig {
    max_entries: 10000,
    max_memory_mb: 256,
    ttl_seconds: 3600,
    eviction_policy: EvictionPolicy::LeastRecentlyUsed,
});

scanner.set_cache_manager(cache_manager);
```

## Error Handling

```rust
use moosicbox_scan::error::ScanError;

match scanner.scan_file(&file_path).await {
    Ok(result) => println!("Scanned: {}", result.track.title),
    Err(ScanError::FileNotFound(path)) => {
        eprintln!("File not found: {}", path);
    },
    Err(ScanError::UnsupportedFormat { file, format }) => {
        eprintln!("Unsupported format {} for file: {}", format, file);
    },
    Err(ScanError::MetadataExtractionFailed { file, error }) => {
        eprintln!("Failed to extract metadata from {}: {}", file, error);
    },
    Err(ScanError::CorruptedFile(file)) => {
        eprintln!("Corrupted file: {}", file);
    },
    Err(ScanError::PermissionDenied(path)) => {
        eprintln!("Permission denied: {}", path);
    },
    Err(ScanError::TimeoutExceeded { file, timeout }) => {
        eprintln!("Timeout scanning {} after {}s", file, timeout);
    },
    Err(e) => {
        eprintln!("Scan error: {}", e);
    }
}
```

## Monitoring and Progress

```rust
use moosicbox_scan::{ScanProgress, ProgressCallback};

async fn scan_with_progress() -> Result<(), Box<dyn std::error::Error>> {
    let progress_callback = |progress: ScanProgress| {
        println!("Progress: {:.1}% ({}/{})",
                 progress.percentage(),
                 progress.completed,
                 progress.total);

        if let Some(current_file) = progress.current_file {
            println!("  Scanning: {}", current_file);
        }

        if progress.errors > 0 {
            println!("  Errors: {}", progress.errors);
        }
    };

    let scanner = Scanner::new(config).await?;
    let result = scanner.scan_with_progress(progress_callback).await?;

    println!("Scan completed: {} tracks processed", result.tracks_added);

    Ok(())
}
```

## Troubleshooting

### Common Issues

1. **Slow scanning**: Reduce parallel jobs, increase chunk size, disable deep analysis
2. **High memory usage**: Reduce cache sizes, enable memory limits
3. **Permission errors**: Check file/directory permissions
4. **Metadata corruption**: Enable strict parsing, verify file integrity

### Debug Information

```bash
# Enable scan debugging
RUST_LOG=moosicbox_scan=debug cargo run

# Scan with verbose output
cargo run --bin scan -- --verbose /path/to/music

# Test specific file
cargo run --bin scan -- --test-file /path/to/audio.mp3

# Performance profiling
cargo run --release --bin scan -- --profile /path/to/music
```

### Performance Analysis

```rust
use moosicbox_scan::{ScanStats, PerformanceProfiler};

// Enable performance profiling
let profiler = PerformanceProfiler::new();
let scanner = Scanner::with_profiler(config, profiler).await?;

let result = scanner.scan().await?;

// Get performance statistics
let stats = scanner.get_performance_stats();
println!("Performance Statistics:");
println!("  Total time: {:.2}s", stats.total_duration.as_secs_f64());
println!("  Files/second: {:.1}", stats.files_per_second);
println!("  Memory peak: {} MB", stats.peak_memory_mb);
println!("  CPU utilization: {:.1}%", stats.avg_cpu_percent);

// Per-format breakdown
for (format, format_stats) in stats.per_format {
    println!("  {}: {:.2}s avg", format, format_stats.avg_duration.as_secs_f64());
}
```

## See Also

- [MoosicBox Library](../library/README.md) - Library management that uses scan results
- [MoosicBox Server](../server/README.md) - Server with scanning API endpoints
- [MoosicBox Files](../files/README.md) - File handling and streaming
