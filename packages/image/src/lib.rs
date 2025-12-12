//! Image resizing and format conversion utilities.
//!
//! This crate provides high-performance image resizing with support for multiple backends
//! and output formats. It offers both synchronous and asynchronous APIs for image manipulation.
//!
//! # Backends
//!
//! * `libvips` - High-performance image processing using libvips (not available on Windows)
//! * `image` - Pure Rust image processing using the `image` crate
//!
//! # Supported Formats
//!
//! * JPEG - Lossy compression with configurable quality
//! * WebP - Modern image format with better compression
//!
//! # Features
//!
//! * `libvips` - Enable libvips backend for high-performance processing
//! * `image` - Enable pure Rust image backend
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "image")]
//! # {
//! # use moosicbox_image::Encoding;
//! use moosicbox_image::image::try_resize_local_file;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Resize an image to 800x600 JPEG with quality 85
//! let resized = try_resize_local_file(
//!     800,
//!     600,
//!     "/path/to/image.jpg",
//!     Encoding::Jpeg,
//!     85,
//! )?;
//! # Ok(())
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use strum_macros::{AsRefStr, Display, EnumString};

/// Image manipulation utilities using the `image` crate.
#[cfg(feature = "image")]
pub mod image;
/// Image manipulation utilities using `libvips`.
#[cfg(not(target_os = "windows"))]
#[cfg(feature = "libvips")]
pub mod libvips;

/// Image encoding format.
#[derive(Debug, Copy, Clone, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Encoding {
    /// JPEG encoding format.
    Jpeg,
    /// WebP encoding format.
    Webp,
}

/// Calculates the target dimensions for an image resize operation.
///
/// Given optional target width and height, along with the original image dimensions,
/// this function calculates the final dimensions to use for resizing:
///
/// * If both width and height are provided, returns them unchanged
/// * If only width is provided, calculates height to maintain aspect ratio
/// * If only height is provided, calculates width to maintain aspect ratio
/// * If neither is provided, returns the original dimensions
#[must_use]
pub fn calculate_target_dimensions(
    width: Option<u32>,
    height: Option<u32>,
    original_dimensions: (u32, u32),
) -> (u32, u32) {
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let h = (f64::from(original_dimensions.1)
                * (f64::from(w) / f64::from(original_dimensions.0)))
            .round() as u32;
            (w, h)
        }
        (None, Some(h)) => {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let w = (f64::from(original_dimensions.0)
                * (f64::from(h) / f64::from(original_dimensions.1)))
            .round() as u32;
            (w, h)
        }
        (None, None) => original_dimensions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn calculate_target_dimensions_both_specified_returns_as_is() {
        let result = calculate_target_dimensions(Some(100), Some(200), (800, 600));
        assert_eq!(result, (100, 200));
    }

    #[test_log::test]
    fn calculate_target_dimensions_only_width_maintains_aspect_ratio() {
        // Original: 800x600 (4:3 ratio)
        // Width: 400 -> Height should be 300 (maintains 4:3)
        let result = calculate_target_dimensions(Some(400), None, (800, 600));
        assert_eq!(result, (400, 300));
    }

    #[test_log::test]
    fn calculate_target_dimensions_only_height_maintains_aspect_ratio() {
        // Original: 800x600 (4:3 ratio)
        // Height: 300 -> Width should be 400 (maintains 4:3)
        let result = calculate_target_dimensions(None, Some(300), (800, 600));
        assert_eq!(result, (400, 300));
    }

    #[test_log::test]
    fn calculate_target_dimensions_neither_specified_returns_original() {
        let result = calculate_target_dimensions(None, None, (800, 600));
        assert_eq!(result, (800, 600));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_16_9_aspect_ratio() {
        // Original: 1920x1080 (16:9 ratio)
        // Width: 1280 -> Height should be 720 (maintains 16:9)
        let result = calculate_target_dimensions(Some(1280), None, (1920, 1080));
        assert_eq!(result, (1280, 720));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_portrait_orientation() {
        // Original: 600x800 (portrait, 3:4 ratio)
        // Width: 300 -> Height should be 400 (maintains 3:4)
        let result = calculate_target_dimensions(Some(300), None, (600, 800));
        assert_eq!(result, (300, 400));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_square_images() {
        // Original: 500x500 (1:1 ratio)
        // Width: 250 -> Height should be 250 (maintains 1:1)
        let result = calculate_target_dimensions(Some(250), None, (500, 500));
        assert_eq!(result, (250, 250));
    }

    #[test_log::test]
    fn calculate_target_dimensions_rounds_fractional_results() {
        // Original: 100x99
        // Width: 50 -> Height should be 50 (49.5 rounds to 50)
        let result = calculate_target_dimensions(Some(50), None, (100, 99));
        assert_eq!(result, (50, 50));

        // Original: 100x101
        // Width: 50 -> Height should be 51 (50.5 rounds to 51)
        let result = calculate_target_dimensions(Some(50), None, (100, 101));
        assert_eq!(result, (50, 51));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_upscaling() {
        // Upscale from 400x300 to width 800
        let result = calculate_target_dimensions(Some(800), None, (400, 300));
        assert_eq!(result, (800, 600));

        // Upscale from 400x300 to height 600
        let result = calculate_target_dimensions(None, Some(600), (400, 300));
        assert_eq!(result, (800, 600));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_extreme_aspect_ratios() {
        // Very wide: 1000x10
        // Height: 5 -> Width should be 500
        let result = calculate_target_dimensions(None, Some(5), (1000, 10));
        assert_eq!(result, (500, 5));

        // Very tall: 10x1000
        // Width: 5 -> Height should be 500
        let result = calculate_target_dimensions(Some(5), None, (10, 1000));
        assert_eq!(result, (5, 500));
    }

    #[test_log::test]
    fn calculate_target_dimensions_handles_same_dimension_as_original() {
        let result = calculate_target_dimensions(Some(800), None, (800, 600));
        assert_eq!(result, (800, 600));

        let result = calculate_target_dimensions(None, Some(600), (800, 600));
        assert_eq!(result, (800, 600));
    }
}
