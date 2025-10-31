# Basic Image Resizing Example

## Summary

This example demonstrates how to use the `moosicbox_image` package to resize images, convert between formats (JPEG and WebP), and control compression quality.

## What This Example Demonstrates

- Resizing images to specific dimensions using `try_resize_local_file`
- Converting between JPEG and WebP encoding formats
- Controlling compression quality for different use cases
- Handling image processing errors
- Comparing file sizes with different quality settings

## Prerequisites

- Basic understanding of Rust programming
- Familiarity with image formats (JPEG, WebP)
- Understanding of image compression and quality tradeoffs

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/image/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will:

1. Create a test image (100x100 gradient pattern)
2. Resize it to 50x50 as JPEG with quality 85
3. Resize it to 80x80 as WebP with quality 90
4. Generate two additional versions with different quality settings (60 and 95)
5. Display the file sizes for comparison

```
MoosicBox Image Basic Usage Example
====================================

Step 1: Creating a test image...
Created test image: test_input.jpg (100x100 pixels)

Step 2: Resizing image to 50x50 JPEG with quality 85...
Successfully resized to JPEG: 825 bytes
Saved to: output_50x50.jpg

Step 3: Resizing image to 80x80 WebP with quality 90...
Successfully resized to WebP: 1234 bytes
Saved to: output_80x80.webp

Step 4: Comparing different quality settings...
Quality 60: 612 bytes
Quality 95: 1156 bytes

All operations completed successfully!
Generated files:
  - output_50x50.jpg (JPEG, quality 85)
  - output_80x80.webp (WebP, quality 90)
  - output_quality_60.jpg (JPEG, quality 60)
  - output_quality_95.jpg (JPEG, quality 95)
```

The generated image files will be created in your current directory.

## Code Walkthrough

### Creating a Test Image

```rust
fn create_test_image(path: &str) -> Result<(), Box<dyn Error>> {
    use image::{ImageBuffer, Rgb};

    // Create a 100x100 RGB image with a gradient pattern
    let img = ImageBuffer::from_fn(100, 100, |x, y| {
        let r = (x as f32 / 100.0 * 255.0) as u8;
        let g = (y as f32 / 100.0 * 255.0) as u8;
        let b = 128;
        Rgb([r, g, b])
    });

    img.save(path)?;
    Ok(())
}
```

This helper function creates a simple test image with a red-green gradient for demonstration purposes.

### Resizing to JPEG

```rust
let resized_jpeg = try_resize_local_file(
    50,             // target width
    50,             // target height
    "test_input.jpg", // input file path
    Encoding::Jpeg,   // output encoding format
    85,             // compression quality (0-100)
)?;
```

The `try_resize_local_file` function takes five parameters:

- **width** and **height**: Target dimensions in pixels
- **path**: Path to the input image file
- **encoding**: Output format (`Encoding::Jpeg` or `Encoding::Webp`)
- **quality**: Compression quality from 0 (lowest) to 100 (highest)

### Resizing to WebP

```rust
let resized_webp = try_resize_local_file(
    80,
    80,
    "test_input.jpg",
    Encoding::Webp,   // WebP format for better compression
    90,
)?;
```

WebP typically provides better compression than JPEG at the same visual quality level, resulting in smaller file sizes.

### Handling the Result

```rust
if let Some(bytes) = resized_jpeg {
    println!("Successfully resized to JPEG: {} bytes", bytes.len());
    std::fs::write("output_50x50.jpg", &bytes)?;
}
```

The function returns `Result<Option<Bytes>, ImageError>`. The `Option` is `None` only if the WebP encoder fails to initialize, which is rare.

## Key Concepts

### Image Resizing

The package uses the Lanczos3 filter for high-quality image resizing. This filter provides excellent results for both upscaling and downscaling operations, preserving image sharpness and detail.

### Encoding Formats

- **JPEG**: Widely supported, good compression for photographs, lossy compression
- **WebP**: Modern format with better compression ratios, supported by most modern browsers

### Quality Settings

Quality is specified as a value from 0 to 100:

- **60-70**: Good for web thumbnails, prioritizes small file size
- **75-85**: Balanced quality and file size for most use cases
- **85-95**: High quality for important images
- **95-100**: Maximum quality, larger files

Higher quality values produce larger files but preserve more visual detail.

### Error Handling

The example uses the `?` operator for error propagation. The function returns `Result<Option<Bytes>, ImageError>`, which can fail if:

- The input file doesn't exist or can't be read
- The input file is not a valid image format
- The image decoding or encoding fails
- There are I/O errors during file operations

## Testing the Example

After running the example, you can verify the results:

1. **Check the generated files**: Four output files should be created in your current directory
2. **Compare file sizes**: Notice how quality settings affect file size
3. **View the images**: Open the files in an image viewer to see the visual quality differences
4. **Modify parameters**: Try changing the dimensions, quality settings, or formats in the code

You can also test with your own images by modifying the code to use a real image file instead of the generated test image.

## Troubleshooting

### "No such file or directory" error

Make sure you're running the example from the repository root, or provide absolute paths to image files.

### WebP encoding returns None

This is rare but can happen if the WebP encoder fails to initialize. The example handles this by checking for `Some(bytes)`.

### Quality parameter has no effect

Ensure you're using values between 0 and 100. Values outside this range will be clamped.

## Related Examples

This is currently the only example for `moosicbox_image`. Future examples may include:

- Asynchronous image processing with `try_resize_local_file_async`
- Batch processing multiple images
- Integration with web servers for dynamic image resizing
- Using the libvips backend for high-performance processing
