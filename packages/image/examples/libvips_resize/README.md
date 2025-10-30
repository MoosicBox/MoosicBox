# Libvips Resize Example

A comprehensive example demonstrating high-performance image resizing using the `moosicbox_image` crate's `libvips` module.

## Summary

This example shows how to use libvips for fast, memory-efficient image resizing operations, including resizing from file paths and byte buffers, creating multiple thumbnail sizes, and proper error handling.

## What This Example Demonstrates

- High-performance image resizing with `resize_local_file`
- Resizing from byte buffers with `resize_bytes`
- Creating multiple thumbnail sizes efficiently
- Error handling with `get_error` for detailed diagnostics
- Automatic color profile management (sRGB conversion)
- Performance characteristics of libvips

## Prerequisites

- **Platform**: Linux or macOS (libvips is not available on Windows)
- **System Dependencies**: libvips must be installed
    - Ubuntu/Debian: `sudo apt-get install libvips-dev`
    - macOS: `brew install vips`
- Basic understanding of image processing concepts

## Running the Example

From the repository root (Linux/macOS only):

```bash
cargo run --manifest-path packages/image/examples/libvips_resize/Cargo.toml
```

On Windows, this example will display an error message directing you to use the `basic_usage` example instead.

## Expected Output

The example creates a temporary directory and demonstrates various libvips operations:

```
Working directory: /tmp/moosicbox_libvips_example

✓ Created test image: /tmp/moosicbox_libvips_example/test_image.png
  Original size: 45678 bytes

--- Example 1: Resize from File Path ---
✓ Resized to 200x150
  Output: /tmp/moosicbox_libvips_example/resized_200x150.jpg
  Size: 5432 bytes

--- Example 2: Resize from Byte Buffer ---
  Input buffer: 45678 bytes
✓ Resized to 100x75 from byte buffer
  Output: /tmp/moosicbox_libvips_example/resized_from_bytes_100x75.jpg
  Size: 2345 bytes

--- Example 3: Multiple Thumbnail Sizes ---
✓ 400x300: 8765 bytes
✓ 200x150: 4321 bytes
✓ 100x75: 2109 bytes
✓ 50x38: 987 bytes

--- Example 4: Performance Characteristics ---
libvips automatically:
  - Uses demand-driven processing (processes only needed pixels)
  - Applies horizontal threading for parallel processing
  - Manages color profiles (converts to sRGB)
  - Uses high-quality interpolation algorithms
  - Minimizes memory usage through streaming

✓ All examples completed successfully!

Output files saved to: /tmp/moosicbox_libvips_example
You can view the resized images in that directory.

--- Example 5: Error Handling ---
✓ Properly caught error for nonexistent file:
  Error: VipsError { ... }
  Libvips details: unable to open file "/nonexistent/path.jpg"
```

## Code Walkthrough

### 1. Resizing from a File Path

The simplest way to resize an image with libvips:

```rust
let resized = moosicbox_image::libvips::resize_local_file(
    200, // target width
    150, // target height
    "/path/to/image.jpg",
)?;
```

This returns a `Result<Bytes, libvips::error::Error>` containing the resized image as JPEG bytes. Libvips automatically:

- Detects the input format
- Applies high-quality Lanczos filtering
- Converts color profiles to sRGB
- Encodes the output as JPEG

### 2. Resizing from a Byte Buffer

When you already have image data in memory:

```rust
let original_bytes = fs::read("/path/to/image.jpg")?;
let resized = moosicbox_image::libvips::resize_bytes(
    100, // width
    75,  // height
    &original_bytes,
)?;
```

This is useful when:

- Loading images from a database
- Processing images from HTTP requests
- Working with images from non-filesystem sources
- Chaining multiple image operations

### 3. Creating Multiple Thumbnails

Libvips excels at batch processing:

```rust
let sizes = [(400, 300), (200, 150), (100, 75), (50, 38)];

for (width, height) in sizes {
    let resized = moosicbox_image::libvips::resize_local_file(
        width,
        height,
        test_image_path.to_str().unwrap(),
    )?;
    // Save each thumbnail
}
```

Each resize operation is independent and benefits from libvips' optimizations.

### 4. Error Handling

Libvips provides detailed error information:

```rust
match moosicbox_image::libvips::resize_local_file(100, 75, "/bad/path.jpg") {
    Ok(_) => { /* success */ },
    Err(e) => {
        println!("Error: {e}");
        // Get additional libvips error details
        let vips_error = moosicbox_image::libvips::get_error();
        if !vips_error.is_empty() {
            println!("Details: {vips_error}");
        }
    }
}
```

The `get_error()` function retrieves and clears the libvips error buffer, providing detailed diagnostic information about what went wrong.

## Key Concepts

### Performance Advantages

Libvips provides significant performance benefits over pure Rust image processing:

1. **Demand-Driven Processing**: Only processes pixels that are actually needed
2. **Horizontal Threading**: Automatically parallelizes operations across CPU cores
3. **Memory Efficiency**: Uses streaming to minimize memory footprint
4. **Optimized Algorithms**: Highly optimized C implementation of image operations

### Color Profile Management

Libvips automatically handles color profiles:

- Input images are converted to sRGB color space
- This ensures consistent color reproduction across different devices
- The `import_profile` and `export_profile` are both set to "sRGB"

### Output Format

Currently, the libvips module outputs JPEG format:

- Encoded with default JPEG quality settings
- Suitable for photographs and general-purpose images
- Good balance of quality and file size

### Thread Safety

The libvips initialization is thread-safe using Rust's `LazyLock`:

```rust
static VIPS: LazyLock<VipsApp> = LazyLock::new(|| {
    VipsApp::new("Moosicbox Libvips", false).expect("Cannot initialize libvips")
});
```

This ensures libvips is initialized exactly once, even in multi-threaded contexts.

## Testing the Example

After running the example:

1. Navigate to the temporary directory shown in the output
2. Open the images in an image viewer
3. Compare the quality of different thumbnail sizes
4. Check file sizes to see the compression efficiency
5. Note the gradient pattern in the test image to evaluate resize quality

## Troubleshooting

### "libvips not found" or similar compilation errors

**Solution**: Install libvips development libraries:

- Ubuntu/Debian: `sudo apt-get install libvips-dev`
- macOS: `brew install vips`
- Fedora: `sudo dnf install vips-devel`

### "This example requires libvips" on Windows

**Reason**: The libvips Rust bindings are not available on Windows.

**Solution**: Use the `basic_usage` example instead, which uses pure Rust image processing and works on all platforms.

### Images appear distorted or incorrect colors

**Cause**: Corrupted source image or incompatible format.

**Solution**: Verify the source image opens correctly in other image viewers. Libvips supports most common formats (JPEG, PNG, WebP, TIFF, etc.).

### Out of memory errors with very large images

**Unlikely**: Libvips is designed to handle large images efficiently through streaming.

**If it occurs**: Check available system memory and consider processing images in smaller batches.

## Related Examples

- `basic_usage` - Pure Rust image processing (cross-platform, including Windows)

## Performance Comparison

For typical use cases:

| Operation                | libvips          | Pure Rust (image crate) |
| ------------------------ | ---------------- | ----------------------- |
| Small images (<1MB)      | Fast             | Fast                    |
| Large images (>10MB)     | Very Fast        | Moderate                |
| Batch processing         | Very Fast        | Moderate                |
| Memory usage (large)     | Low (streaming)  | High (full buffer)      |
| Platform support         | Linux/macOS only | All platforms           |
| Dependencies             | System library   | Pure Rust               |
| Ease of deployment       | Moderate         | Easy                    |
| Processing quality       | Excellent        | Excellent               |
| Parallel processing      | Automatic        | Manual                  |
| Color profile management | Automatic        | Manual                  |

**Recommendation**:

- Use **libvips** for server applications processing many/large images
- Use **image crate** for cross-platform tools, small images, or when deployment simplicity is important
