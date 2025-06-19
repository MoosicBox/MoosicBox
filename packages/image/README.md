# MoosicBox Image Helper

A command-line image processing tool for resizing, converting, and optimizing images with support for multiple formats.

## Overview

The Image Helper (`image_helper`) is a utility for image processing operations including:

- **Image Resizing**: Resize images while maintaining aspect ratio
- **Format Conversion**: Convert between multiple image formats
- **Quality Control**: Configurable compression quality settings
- **Smart Resizing**: Automatic aspect ratio calculation
- **Batch Processing**: Process multiple images efficiently
- **libvips Integration**: High-performance image processing (when available)

## Installation

### From Source

```bash
cargo install --path packages/image --features "build-binary,image,libvips"
```

### Dependencies

System dependencies for optimal performance:

- **libvips** (recommended, for high-performance processing)
  - Ubuntu/Debian: `sudo apt-get install libvips-dev`
  - macOS: `brew install vips`
  - Windows: See [libvips Windows installation](https://www.libvips.org/install.html)

## Usage

### Basic Usage

Resize an image to specific dimensions:
```bash
image_helper input.jpg output.jpg --width 800 --height 600
```

### Maintain Aspect Ratio

Resize by width only (height calculated automatically):
```bash
image_helper input.png output.png --width 1024
```

Resize by height only (width calculated automatically):
```bash
image_helper large.jpg thumbnail.jpg --height 200
```

### Format Conversion

Convert between formats:
```bash
image_helper photo.png photo.jpg --encoding JPEG --quality 85
```

### Quality Control

Set compression quality (0-100):
```bash
image_helper input.jpg output.jpg --width 800 --quality 95
```

### Complete Example

```bash
image_helper \
  /path/to/input.png \
  /path/to/output.webp \
  --width 1200 \
  --height 800 \
  --encoding WEBP \
  --quality 90
```

## Command Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--width` | `-w` | Target width in pixels | Auto-calculated |
| `--height` | `-h` | Target height in pixels | Auto-calculated |
| `--encoding` | `-e` | Output format (JPEG, PNG, WEBP) | Auto-detect from extension |
| `--output` | `-o` | Output file path | Required |
| `--quality` | `-q` | Compression quality (0-100) | 80 |

## Supported Formats

### Input Formats
- **JPEG** (.jpg, .jpeg)
- **PNG** (.png)
- **WebP** (.webp)
- **TIFF** (.tiff, .tif)
- **BMP** (.bmp)
- **GIF** (.gif) - static images only
- **ICO** (.ico)

### Output Formats
- **JPEG** (.jpg, .jpeg) - Good compression, lossy
- **PNG** (.png) - Lossless, supports transparency
- **WebP** (.webp) - Modern format, excellent compression

## Aspect Ratio Handling

The tool intelligently handles aspect ratios:

### Both Dimensions Specified
```bash
# Resize to exact dimensions (may distort image)
image_helper input.jpg output.jpg --width 800 --height 600
```

### Width Only
```bash
# Height calculated to maintain aspect ratio
image_helper input.jpg output.jpg --width 800
```

### Height Only
```bash
# Width calculated to maintain aspect ratio
image_helper input.jpg output.jpg --height 600
```

### Neither Dimension
```bash
# Original dimensions preserved, format/quality change only
image_helper input.png output.jpg --encoding JPEG --quality 85
```

## Quality Guidelines

### JPEG Quality Settings
- **60-70**: Good for web thumbnails, small file size
- **75-85**: Good balance of quality and file size
- **85-95**: High quality for photos
- **95-100**: Maximum quality, larger files

### WebP Quality Settings
- **50-70**: Excellent compression for web use
- **70-85**: High quality with good compression
- **85-100**: Maximum quality

### PNG
- PNG is lossless, quality setting affects compression level
- Higher quality = better compression but slower processing

## Examples

### Create web thumbnails
```bash
# Create small thumbnail
image_helper photo.jpg thumb.jpg --width 150 --quality 75

# Create medium preview
image_helper photo.jpg preview.jpg --width 400 --quality 80
```

### Convert to modern formats
```bash
# Convert PNG to WebP for better compression
image_helper large.png optimized.webp --quality 85

# Convert old JPEG to high-quality WebP
image_helper old-photo.jpg new-photo.webp --quality 90
```

### Batch processing script
```bash
#!/bin/bash
mkdir -p thumbnails
for img in *.jpg; do
  image_helper "$img" "thumbnails/${img%.jpg}_thumb.jpg" --width 200 --quality 80
done
```

### Album artwork optimization
```bash
# Standard album cover size
image_helper cover.png cover.jpg --width 1000 --height 1000 --quality 90

# High-res album cover
image_helper cover.png cover_hd.jpg --width 1400 --height 1400 --quality 95
```

## Performance

### libvips vs Image Crate
- **libvips**: Faster processing, better memory usage, more formats
- **image crate**: Pure Rust, easier deployment, fewer dependencies

### Memory Usage
- Optimized for large images with streaming processing
- Automatically handles memory management for batch operations

### Processing Speed
- Multi-threaded processing where supported
- Efficient algorithms for common operations like resizing

## Error Handling

Common errors and solutions:

1. **Unsupported format**: Check input file format is supported
2. **Permission denied**: Verify read/write permissions for files
3. **Out of memory**: Reduce image size or enable libvips for better memory usage
4. **Invalid dimensions**: Ensure width/height values are positive integers

## Integration

The image helper is used by other MoosicBox components:

- **Server**: Automatic album artwork optimization
- **Web Interface**: Dynamic image resizing for different screen sizes
- **Mobile App**: Thumbnail generation for better performance

## See Also

- [MoosicBox Server](../server/README.md) - Uses image processing for album artwork
- [MoosicBox Files](../files/README.md) - File handling utilities
- [libvips Documentation](https://www.libvips.org/) - High-performance image processing library
