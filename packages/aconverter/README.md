# MoosicBox Audio Converter (aconverter)

A command-line audio format converter that supports multiple audio formats with metadata preservation.

## Overview

The Audio Converter (`aconverter`) is a utility that converts audio files between different formats while preserving metadata tags. It supports:

- **Multiple Input Formats**: MP3, FLAC, AAC/M4A, Opus, and more
- **Multiple Output Formats**: AAC, FLAC, MP3, Opus
- **Metadata Preservation**: Automatically copies tags from source to output
- **Single File Conversion**: Converts one file at a time (batch processing can be scripted)

## Installation

### From Source

```bash
cargo install --path packages/aconverter --features "aac,flac,mp3,opus"
```

### Dependencies

This package uses the MoosicBox audio encoding infrastructure. The actual encoding is handled by the `moosicbox_files` and `moosicbox_audio_output` packages. No additional system dependencies are required beyond Rust and Cargo.

## Usage

### Basic Usage

Convert a single audio file:

```bash
aconverter input.flac --output output.mp3
```

### Specify Output Format

Explicitly specify the output format:

```bash
aconverter input.mp3 --output output.flac --encoding FLAC
```

### Quality Settings

**Note**: The quality parameter is accepted but not currently implemented in the encoding process. It will be parsed but has no effect on the output.

```bash
# Quality parameter is accepted but does not affect encoding
aconverter input.wav --output output.mp3 --quality 95
```

### Complete Example

```bash
aconverter \
  /path/to/input.flac \
  --output /path/to/output.mp3 \
  --encoding MP3 \
  --quality 90
```

## Command Line Options

| Option       | Short | Description                                          | Default                    |
| ------------ | ----- | ---------------------------------------------------- | -------------------------- |
| `--encoding` | `-e`  | Output format (AAC, FLAC, MP3, OPUS)                 | Auto-detect from extension |
| `--output`   | `-o`  | Output file path                                     | Required                   |
| `--quality`  | `-q`  | Encoding quality (0-100) (currently not implemented) | 80                         |
| `--width`    |       | Width parameter (accepted but not used)              | None                       |
| `--height`   |       | Height parameter (accepted but not used)             | None                       |

## Supported Formats

### Input Formats

- **MP3** (.mp3)
- **FLAC** (.flac)
- **AAC/M4A** (.aac, .m4a, .mp4)
- **Opus** (.opus)
- **WAV** (.wav)
- **OGG** (.ogg)

### Output Formats

- **AAC** (.aac, .m4a) - Advanced Audio Codec
- **FLAC** (.flac) - Free Lossless Audio Codec
- **MP3** (.mp3) - MPEG Layer 3
- **Opus** (.opus) - Modern low-latency codec

## Metadata Support

The converter automatically preserves the following metadata tags:

- **Title** - Track title
- **Artist** - Track artist
- **Album** - Album name
- **Album Artist** - Album artist
- **Track Number** - Track position
- **Date/Year** - Release date

**Note**: Genre and embedded artwork preservation are not currently implemented.

## Quality Guidelines

**Note**: The quality parameter is currently not implemented in the encoding process. The information below describes planned functionality.

### Lossless Formats

- **FLAC**: Perfect quality preservation, larger file size
- Use for archival purposes or when quality is paramount

### Lossy Formats

- **AAC**: Advanced Audio Codec, good for streaming
- **MP3**: Widely compatible format
- **Opus**: Modern low-latency codec

**Note**: Quality control for lossy formats is planned but not yet implemented. The current implementation uses default encoding settings.

## Examples

### Convert FLAC to MP3

```bash
aconverter album.flac --output album.mp3
```

### Convert MP3 to lossless FLAC

```bash
aconverter song.mp3 --output song.flac
```

### Batch convert with shell script

```bash
#!/bin/bash
for file in *.flac; do
  aconverter "$file" --output "${file%.flac}.mp3"
done
```

### Convert with format specification

```bash
aconverter input.flac --output output.m4a --encoding AAC
```

## Error Handling

Common errors and solutions:

1. **Unsupported format**: Ensure the input format is supported and the appropriate feature flags were enabled during installation
2. **Permission denied**: Check file permissions for input and output paths
3. **Encoding failed**: Verify the output format is supported (check Cargo features)
4. **Metadata read error**: Some files may have corrupted or unsupported metadata

## Performance

- **Async I/O**: Uses asynchronous I/O (Tokio runtime) for efficient processing
- **Streaming**: Processes audio data as a stream to handle large files efficiently
- **Disk space**: Ensure sufficient space for output files (lossless formats are larger)

## Technical Details

The converter uses the following MoosicBox packages for audio processing:

- **moosicbox_files**: Provides the core file handling and audio conversion functionality through the `get_audio_bytes` function
- **moosicbox_audio_output**: Handles the actual audio encoding for supported formats
- **moosicbox_audiotags**: Manages metadata reading and writing using the `Tag` API

## See Also

- [MoosicBox Files](../files/README.md) - File handling and audio conversion infrastructure
- [MoosicBox Server](../server/README.md) - Main music server with format support
