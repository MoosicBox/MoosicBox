# MoosicBox Audio Converter (aconverter)

A command-line audio format converter that supports multiple audio formats with metadata preservation.

## Overview

The Audio Converter (`aconverter`) is a utility that converts audio files between different formats while preserving metadata tags. It supports:

- **Multiple Input Formats**: MP3, FLAC, AAC/M4A, Opus, and more
- **Multiple Output Formats**: AAC, FLAC, MP3, Opus
- **Metadata Preservation**: Automatically copies tags from source to output
- **Quality Control**: Configurable encoding quality settings
- **Batch Processing**: Convert multiple files efficiently

## Installation

### From Source

```bash
cargo install --path packages/aconverter --features "aac,flac,mp3,opus"
```

### Dependencies

The following system dependencies may be required depending on the formats you want to support:

- **libvorbis-dev** (for Opus support)
- **libopus-dev** (for Opus support)
- **libaac-dev** (for AAC support)

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

**Note**: Quality parameter is parsed but not currently implemented in the encoding process.

Set encoding quality (0-100, default 80):

```bash
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
| `--width`    |       | Output width (currently not implemented)             | None                       |
| `--height`   |       | Output height (currently not implemented)            | None                       |

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

### Lossy Formats (Planned)

- **AAC**: Excellent quality at lower bitrates, good for streaming
    - Quality 80-90: Good for general listening
    - Quality 90-100: High quality for critical listening
- **MP3**: Widely compatible, good quality
    - Quality 70-80: Good for portable devices
    - Quality 80-95: High quality for most uses
- **Opus**: Best quality per bitrate, modern codec
    - Quality 60-80: Excellent for voice/music streaming
    - Quality 80-95: High quality music

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

1. **Unsupported format**: Ensure the input format is supported
2. **Permission denied**: Check file permissions for input and output paths
3. **Encoding failed**: Verify system dependencies are installed
4. **Metadata read error**: Some files may have corrupted or unsupported metadata

## Performance

- **Async I/O**: The converter uses asynchronous I/O for efficient processing
- **Memory usage**: Optimized for large files with streaming processing
- **Disk space**: Ensure sufficient space for output files (lossless formats are larger)

## See Also

- [MoosicBox Server](../server/README.md) - Main music server with format support
- [MoosicBox Image Helper](../image/README.md) - Image processing utilities
- [MoosicBox Files](../files/README.md) - File handling utilities
