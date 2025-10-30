#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! High-performance image resizing example using libvips.
//!
//! This example demonstrates how to use the `libvips` module for fast,
//! memory-efficient image resizing. Libvips is significantly faster than
//! pure Rust implementations for large images.

#[cfg(not(target_os = "windows"))]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::path::Path;

#[cfg(not(target_os = "windows"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for our examples
    let temp_dir = std::env::temp_dir().join("moosicbox_libvips_example");
    fs::create_dir_all(&temp_dir)?;
    println!("Working directory: {}", temp_dir.display());

    // Create a test image
    let test_image_path = temp_dir.join("test_image.png");
    create_test_image(&test_image_path)?;
    println!("\n✓ Created test image: {}", test_image_path.display());
    println!(
        "  Original size: {} bytes",
        fs::metadata(&test_image_path)?.len()
    );

    // Example 1: Resize from file path
    println!("\n--- Example 1: Resize from File Path ---");
    let resized = moosicbox_image::libvips::resize_local_file(
        200, // width
        150, // height
        test_image_path.to_str().unwrap(),
    )?;

    let output_path = temp_dir.join("resized_200x150.jpg");
    fs::write(&output_path, resized.as_ref())?;
    println!("✓ Resized to 200x150");
    println!("  Output: {}", output_path.display());
    println!("  Size: {} bytes", fs::metadata(&output_path)?.len());

    // Example 2: Resize from byte buffer
    println!("\n--- Example 2: Resize from Byte Buffer ---");
    let original_bytes = fs::read(&test_image_path)?;
    println!("  Input buffer: {} bytes", original_bytes.len());

    let resized_from_bytes = moosicbox_image::libvips::resize_bytes(
        100, // width
        75,  // height
        &original_bytes,
    )?;

    let output_path = temp_dir.join("resized_from_bytes_100x75.jpg");
    fs::write(&output_path, resized_from_bytes.as_ref())?;
    println!("✓ Resized to 100x75 from byte buffer");
    println!("  Output: {}", output_path.display());
    println!("  Size: {} bytes", fs::metadata(&output_path)?.len());

    // Example 3: Create multiple thumbnails
    println!("\n--- Example 3: Multiple Thumbnail Sizes ---");
    let sizes = [(400, 300), (200, 150), (100, 75), (50, 38)];

    for (width, height) in sizes {
        let resized = moosicbox_image::libvips::resize_local_file(
            width,
            height,
            test_image_path.to_str().unwrap(),
        )?;

        let output_path = temp_dir.join(format!("thumbnail_{width}x{height}.jpg"));
        fs::write(&output_path, resized.as_ref())?;
        let file_size = fs::metadata(&output_path)?.len();
        println!("✓ {width}x{height}: {file_size} bytes");
    }

    // Example 4: Compare performance characteristics
    println!("\n--- Example 4: Performance Characteristics ---");
    println!("libvips automatically:");
    println!("  - Uses demand-driven processing (processes only needed pixels)");
    println!("  - Applies horizontal threading for parallel processing");
    println!("  - Manages color profiles (converts to sRGB)");
    println!("  - Uses high-quality interpolation algorithms");
    println!("  - Minimizes memory usage through streaming");

    println!("\n✓ All examples completed successfully!");
    println!("\nOutput files saved to: {}", temp_dir.display());
    println!("You can view the resized images in that directory.");

    // Demonstrate error handling
    println!("\n--- Example 5: Error Handling ---");
    match moosicbox_image::libvips::resize_local_file(100, 75, "/nonexistent/path.jpg") {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            println!("✓ Properly caught error for nonexistent file:");
            println!("  Error: {e}");
            // Get detailed libvips error
            let vips_error = moosicbox_image::libvips::get_error();
            if !vips_error.is_empty() {
                println!("  Libvips details: {vips_error}");
            }
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn create_test_image(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use image::{ImageBuffer, Rgb};

    // Create a larger test image (400x300) to better demonstrate libvips performance
    let width = 400;
    let height = 300;
    let mut img = ImageBuffer::new(width, height);

    // Create a gradient pattern
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let r = ((x as f32 / width as f32) * 255.0) as u8;
        let g = ((y as f32 / height as f32) * 255.0) as u8;
        let b = 128;
        *pixel = Rgb([r, g, b]);
    }

    img.save(path)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn main() {
    eprintln!("This example requires libvips, which is not available on Windows.");
    eprintln!(
        "Please use the 'basic_usage' example instead, which uses pure Rust image processing."
    );
    std::process::exit(1);
}
