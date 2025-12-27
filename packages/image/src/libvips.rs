//! Image manipulation utilities using `libvips`.
//!
//! This module provides high-performance image processing using the libvips library.
//! Libvips is a demand-driven, horizontally threaded image processing library that
//! is significantly faster than pure Rust implementations for large images.
//!
//! # Features
//!
//! * High-performance image resizing from files
//! * Resize from byte buffers
//! * Error handling utilities for libvips
//! * Automatic color profile management (sRGB)
//! * Thread-safe operations with lazy initialization
//!
//! # Platform Support
//!
//! This module is not available on Windows platforms.
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(all(not(target_os = "windows"), feature = "libvips"))]
//! # {
//! use moosicbox_image::libvips::resize_local_file;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Resize an image to 800x600
//! let resized = resize_local_file(800, 600, "/path/to/image.jpg")?;
//! # Ok(())
//! # }
//! # }
//! ```

use std::sync::LazyLock;

use bytes::Bytes;
use libvips::{VipsApp, VipsImage, ops};
use log::debug;

static VIPS: LazyLock<VipsApp> = LazyLock::new(|| {
    debug!("Initializing libvips");
    let app = VipsApp::new("Moosicbox Libvips", false).expect("Cannot initialize libvips");

    app.concurrency_set(4);

    app
});

/// Gets and clears the current libvips error buffer.
///
/// # Panics
///
/// Panics if the libvips library cannot be initialized on first access.
#[must_use]
pub fn get_error() -> String {
    let error = VIPS.error_buffer().unwrap_or_default().to_string();
    VIPS.error_clear();
    error
}

/// Resizes an image file using libvips.
///
/// # Errors
///
/// * [`libvips::error::Error`] - If the image file cannot be loaded
/// * [`libvips::error::Error`] - If the thumbnail generation fails
/// * [`libvips::error::Error`] - If the image encoding to JPEG fails
///
/// # Panics
///
/// Panics if the libvips library cannot be initialized on first access.
pub fn resize_local_file(
    width: u32,
    height: u32,
    path: &str,
) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailImageOptions {
        #[allow(clippy::cast_possible_wrap)]
        height: height as i32,
        input_profile: "sRGB".into(),
        output_profile: "sRGB".into(),
        ..ops::ThumbnailImageOptions::default()
    };

    let image = VipsImage::new_from_file(path)?;

    #[allow(clippy::cast_possible_wrap)]
    let thumbnail = ops::thumbnail_image_with_opts(&image, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}

/// Resizes image data from a byte buffer using libvips.
///
/// # Errors
///
/// * [`libvips::error::Error`] - If the image buffer cannot be decoded
/// * [`libvips::error::Error`] - If the thumbnail generation fails
/// * [`libvips::error::Error`] - If the image encoding to JPEG fails
///
/// # Panics
///
/// Panics if the libvips library cannot be initialized on first access.
pub fn resize_bytes(width: u32, height: u32, bytes: &[u8]) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailBufferOptions {
        #[allow(clippy::cast_possible_wrap)]
        height: height as i32,
        input_profile: "sRGB".into(),
        output_profile: "sRGB".into(),
        ..ops::ThumbnailBufferOptions::default()
    };

    #[allow(clippy::cast_possible_wrap)]
    let thumbnail = ops::thumbnail_buffer_with_opts(bytes, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}
