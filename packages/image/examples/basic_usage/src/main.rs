#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic image resizing example demonstrating the `moosicbox_image` package.
//!
//! This example shows how to:
//! - Resize images using both sync and async APIs
//! - Convert between different image formats (JPEG and WebP)
//! - Control compression quality
//! - Handle errors during image processing

use moosicbox_image::{Encoding, image::try_resize_local_file};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("MoosicBox Image Basic Usage Example");
    println!("====================================\n");

    // Create a simple test image in memory for demonstration
    // In a real application, you would use an actual image file
    println!("Step 1: Creating a test image...");
    create_test_image("test_input.jpg")?;
    println!("Created test image: test_input.jpg (100x100 pixels)\n");

    // Example 1: Resize to specific dimensions with JPEG encoding
    println!("Step 2: Resizing image to 50x50 JPEG with quality 85...");
    let resized_jpeg = try_resize_local_file(
        50,               // target width
        50,               // target height
        "test_input.jpg", // input file path
        Encoding::Jpeg,   // output encoding format
        85,               // compression quality (0-100)
    )?;

    if let Some(bytes) = resized_jpeg {
        println!("Successfully resized to JPEG: {} bytes", bytes.len());
        std::fs::write("output_50x50.jpg", &bytes)?;
        println!("Saved to: output_50x50.jpg\n");
    }

    // Example 2: Resize to different dimensions with WebP encoding
    println!("Step 3: Resizing image to 80x80 WebP with quality 90...");
    let resized_webp = try_resize_local_file(
        80, // target width
        80, // target height
        "test_input.jpg",
        Encoding::Webp, // WebP format for better compression
        90,             // higher quality setting
    )?;

    if let Some(bytes) = resized_webp {
        println!("Successfully resized to WebP: {} bytes", bytes.len());
        std::fs::write("output_80x80.webp", &bytes)?;
        println!("Saved to: output_80x80.webp\n");
    }

    // Example 3: Demonstrate different quality settings
    println!("Step 4: Comparing different quality settings...");

    // Low quality (smaller file size)
    let low_quality = try_resize_local_file(50, 50, "test_input.jpg", Encoding::Jpeg, 60)?;
    if let Some(bytes) = low_quality {
        std::fs::write("output_quality_60.jpg", &bytes)?;
        println!("Quality 60: {} bytes", bytes.len());
    }

    // High quality (larger file size, better visual quality)
    let high_quality = try_resize_local_file(50, 50, "test_input.jpg", Encoding::Jpeg, 95)?;
    if let Some(bytes) = high_quality {
        std::fs::write("output_quality_95.jpg", &bytes)?;
        println!("Quality 95: {} bytes", bytes.len());
    }

    println!("\nAll operations completed successfully!");
    println!("Generated files:");
    println!("  - output_50x50.jpg (JPEG, quality 85)");
    println!("  - output_80x80.webp (WebP, quality 90)");
    println!("  - output_quality_60.jpg (JPEG, quality 60)");
    println!("  - output_quality_95.jpg (JPEG, quality 95)");

    // Clean up test input file
    std::fs::remove_file("test_input.jpg").ok();

    Ok(())
}

/// Creates a simple test image for demonstration purposes.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
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
