# Basic Usage Example

A comprehensive example demonstrating image resizing and format conversion using the `moosicbox_image` crate's pure Rust `image` module.

## Summary

This example shows how to resize images to different dimensions, convert between formats (JPEG, WebP), adjust quality settings, and use both synchronous and asynchronous APIs for image processing.

## What This Example Demonstrates

- Creating test images programmatically for demonstration
- Synchronous image resizing with `try_resize_local_file`
- Asynchronous image resizing with `try_resize_local_file_async`
- Converting between image formats (PNG to JPEG, PNG to WebP)
- Adjusting compression quality settings
- Comparing output file sizes at different quality levels
- Working with the `Encoding` enum (JPEG and WebP formats)

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with image formats (JPEG, PNG, WebP)
- Understanding of image quality and compression concepts

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/image/examples/basic_usage/Cargo.toml
```

With WebP support enabled:

```bash
cargo run --manifest-path packages/image/examples/basic_usage/Cargo.toml --features webp
```

## Expected Output

The example will create a temporary directory and output progress messages:

```
Working directory: /tmp/moosicbox_image_example

✓ Created test image: /tmp/moosicbox_image_example/test_image.png

--- Example 1: Synchronous Resize to JPEG ---
✓ Resized to 50x50 JPEG (quality 85)
  Output: /tmp/moosicbox_image_example/resized_50x50.jpg
  Size: 1234 bytes

--- Example 2: Async Resize to Different Dimensions ---
✓ Async resized to 80x60 JPEG (quality 90)
  Output: /tmp/moosicbox_image_example/resized_80x60.jpg
  Size: 2345 bytes

--- Example 3: Convert to WebP ---
✓ Converted to WebP (quality 80)
  Output: /tmp/moosicbox_image_example/converted.webp
  Size: 1567 bytes

--- Example 4: Quality Comparison ---
✓ Quality 50: 1100 bytes
✓ Quality 75: 1500 bytes
✓ Quality 95: 2200 bytes

✓ All examples completed successfully!

Output files saved to: /tmp/moosicbox_image_example
You can view the resized images in that directory.
```

The example creates several output files that you can inspect to see the effects of different resize operations and quality settings.

## Code Walkthrough

### 1. Creating a Test Image

The example starts by creating a simple 100x100 red square as a test image:

```rust
use image::{ImageBuffer, Rgb};

let mut img = ImageBuffer::new(100, 100);
for pixel in img.pixels_mut() {
    *pixel = Rgb([255, 0, 0]);
}
img.save(path)?;
```

This demonstrates using the `image` crate to create test data programmatically, which is useful for testing and examples.

### 2. Synchronous Resize

The first example shows the synchronous API:

```rust
let resized_jpeg = moosicbox_image::image::try_resize_local_file(
    50,  // target width
    50,  // target height
    test_image_path.to_str().unwrap(),
    Encoding::Jpeg,
    85, // quality (0-100)
)?;
```

This function blocks until the resize operation completes and returns `Result<Option<Bytes>, ImageError>`. The `Option` is `None` if encoding fails for format-specific reasons.

### 3. Asynchronous Resize

For async contexts, use `try_resize_local_file_async`:

```rust
let resized_async = moosicbox_image::image::try_resize_local_file_async(
    80,  // width
    60,  // height
    test_image_path.to_str().unwrap(),
    Encoding::Jpeg,
    90, // higher quality
)
.await?;
```

This offloads the CPU-intensive image processing to a blocking thread pool, preventing it from blocking the async runtime. This is the recommended approach for server applications.

### 4. Format Conversion

Converting to WebP demonstrates the format flexibility:

```rust
let webp_result = moosicbox_image::image::try_resize_local_file(
    100, // keep original dimensions
    100,
    test_image_path.to_str().unwrap(),
    Encoding::Webp,
    80, // WebP quality
)?;
```

WebP typically provides better compression than JPEG at equivalent quality levels.

### 5. Quality Comparison

The example compares output sizes at different quality levels:

```rust
for quality in [50, 75, 95] {
    let result = moosicbox_image::image::try_resize_local_file(
        100, 100,
        test_image_path.to_str().unwrap(),
        Encoding::Jpeg,
        quality,
    )?;
    // Save and compare file sizes
}
```

This helps you understand the tradeoff between quality and file size.

## Key Concepts

### Image Quality

Quality is a value from 0-100:

- **50-70**: Good for web thumbnails where file size is critical
- **75-85**: Balanced quality and size, suitable for most web use
- **85-95**: High quality for photos and important images
- **95-100**: Maximum quality, minimal compression artifacts

### Encoding Formats

- **JPEG**: Lossy compression, good for photographs, widely supported
- **WebP**: Modern format with better compression, but less universal support

### Synchronous vs Asynchronous

- **Synchronous** (`try_resize_local_file`): Simple, blocks current thread, good for CLI tools or batch processing
- **Asynchronous** (`try_resize_local_file_async`): Non-blocking, good for servers and concurrent applications

### Filter Quality

The `image` module uses Lanczos3 filtering internally, which provides high-quality downscaling with good sharpness preservation. This is a good default for most use cases.

## Testing the Example

After running the example:

1. Navigate to the temporary directory shown in the output
2. Open the images in an image viewer to compare results
3. Check file sizes to see the impact of quality settings
4. Compare JPEG vs WebP output (if WebP feature enabled)

## Troubleshooting

### "No such file or directory" error

- Ensure you have write permissions to `/tmp` (or your system's temp directory)
- The example creates its own test image, so no input files are needed

### WebP example not running

- Enable the `webp` feature: `--features webp`
- The WebP feature requires the `webp` crate dependency

### Out of memory errors

- This example uses small test images (100x100), so memory issues are unlikely
- For large images in your own code, consider the async API to better manage resources

## Related Examples

- `libvips_resize` - High-performance resizing using libvips (Linux/macOS only)
