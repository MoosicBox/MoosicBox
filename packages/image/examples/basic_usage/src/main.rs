#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_image`.
//!
//! This example demonstrates how to use the `image` module to resize images
//! in various formats with different quality settings.

use moosicbox_image::Encoding;
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for our examples
    let temp_dir = std::env::temp_dir().join("moosicbox_image_example");
    fs::create_dir_all(&temp_dir)?;
    println!("Working directory: {}", temp_dir.display());

    // Create a simple test image (a small colored square)
    // This creates a 100x100 red square in PNG format
    let test_image_path = temp_dir.join("test_image.png");
    create_test_image(&test_image_path)?;
    println!("\n✓ Created test image: {}", test_image_path.display());

    // Example 1: Synchronous resize to JPEG
    println!("\n--- Example 1: Synchronous Resize to JPEG ---");
    let resized_jpeg = moosicbox_image::image::try_resize_local_file(
        50, // width
        50, // height
        test_image_path.to_str().unwrap(),
        Encoding::Jpeg,
        85, // quality (0-100)
    )?;

    if let Some(bytes) = resized_jpeg {
        let output_path = temp_dir.join("resized_50x50.jpg");
        fs::write(&output_path, bytes.as_ref())?;
        println!("✓ Resized to 50x50 JPEG (quality 85)");
        println!("  Output: {}", output_path.display());
        println!("  Size: {} bytes", fs::metadata(&output_path)?.len());
    }

    // Example 2: Async resize to different dimensions
    println!("\n--- Example 2: Async Resize to Different Dimensions ---");
    let resized_async = moosicbox_image::image::try_resize_local_file_async(
        80, // width
        60, // height
        test_image_path.to_str().unwrap(),
        Encoding::Jpeg,
        90, // higher quality
    )
    .await?;

    if let Some(bytes) = resized_async {
        let output_path = temp_dir.join("resized_80x60.jpg");
        fs::write(&output_path, bytes.as_ref())?;
        println!("✓ Async resized to 80x60 JPEG (quality 90)");
        println!("  Output: {}", output_path.display());
        println!("  Size: {} bytes", fs::metadata(&output_path)?.len());
    }

    // Example 3: Convert to WebP format
    #[cfg(feature = "webp")]
    {
        println!("\n--- Example 3: Convert to WebP ---");
        let webp_result = moosicbox_image::image::try_resize_local_file(
            100, // keep original width
            100, // keep original height
            test_image_path.to_str().unwrap(),
            Encoding::Webp,
            80, // WebP quality
        )?;

        if let Some(bytes) = webp_result {
            let output_path = temp_dir.join("converted.webp");
            fs::write(&output_path, bytes.as_ref())?;
            println!("✓ Converted to WebP (quality 80)");
            println!("  Output: {}", output_path.display());
            println!("  Size: {} bytes", fs::metadata(&output_path)?.len());
        }
    }

    // Example 4: Quality comparison
    println!("\n--- Example 4: Quality Comparison ---");
    for quality in [50, 75, 95] {
        let result = moosicbox_image::image::try_resize_local_file(
            100,
            100,
            test_image_path.to_str().unwrap(),
            Encoding::Jpeg,
            quality,
        )?;

        if let Some(bytes) = result {
            let output_path = temp_dir.join(format!("quality_{quality}.jpg"));
            fs::write(&output_path, bytes.as_ref())?;
            println!(
                "✓ Quality {quality}: {} bytes",
                fs::metadata(&output_path)?.len()
            );
        }
    }

    println!("\n✓ All examples completed successfully!");
    println!("\nOutput files saved to: {}", temp_dir.display());
    println!("You can view the resized images in that directory.");

    Ok(())
}

/// Creates a simple test image (100x100 red square) for demonstration purposes.
fn create_test_image(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use image::{ImageBuffer, Rgb};

    // Create a 100x100 RGB image with u8 pixels
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(100, 100);

    // Fill with a red color
    for pixel in img.pixels_mut() {
        *pixel = Rgb([255u8, 0u8, 0u8]);
    }

    // Save as PNG
    img.save(path)?;
    Ok(())
}
