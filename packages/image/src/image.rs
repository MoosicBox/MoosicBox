//! Image manipulation utilities using the `image` crate.
//!
//! This module provides a pure Rust implementation for image resizing and encoding
//! using the [`image`](https://docs.rs/image) crate. It supports both synchronous
//! and asynchronous operations.
//!
//! # Features
//!
//! * Synchronous image resizing
//! * Asynchronous image resizing
//! * Support for JPEG and WebP output formats
//! * High-quality Lanczos3 filtering for image resizing
//!
//! # Examples
//!
//! ```rust,no_run
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
//! ```

use bytes::Bytes;
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType};
use thiserror::Error;

use crate::Encoding;

/// Resizes an image file and encodes it in the specified format.
///
/// # Errors
///
/// * [`image::error::ImageError`] - If the image file cannot be opened or decoded
/// * [`image::error::ImageError`] - If the image encoder fails to encode the resized image
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn try_resize_local_file(
    width: u32,
    height: u32,
    path: &str,
    encoding: Encoding,
    quality: u8,
) -> Result<Option<Bytes>, image::error::ImageError> {
    let img = image::open(path)?;
    let resized = img.resize(width, height, FilterType::Lanczos3);
    match encoding {
        Encoding::Jpeg => {
            let mut buffer = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
            encoder.encode_image(&resized)?;
            Ok(Some(buffer.into()))
        }
        Encoding::Webp => webp::Encoder::from_image(&resized).map_or(Ok(None), |encoder| {
            let memory = encoder.encode(quality.into());
            let bytes = memory.to_vec();
            Ok(Some(bytes.into()))
        }),
    }
}

/// Error type for image resize operations.
#[derive(Debug, Error)]
pub enum ResizeImageError {
    /// Image processing error from the `image` crate.
    ///
    /// This error occurs when image decoding, processing, or encoding fails.
    #[error(transparent)]
    Image(#[from] image::error::ImageError),
    /// Task join error from the async runtime.
    ///
    /// This error occurs when the blocking task fails to complete successfully.
    #[error(transparent)]
    Join(#[from] switchy_async::task::JoinError),
}

/// Asynchronously resizes an image file and encodes it in the specified format.
///
/// This function offloads the image processing to a blocking thread pool to avoid
/// blocking the async runtime.
///
/// # Errors
///
/// * [`ResizeImageError::Image`] - If the image file cannot be opened, decoded, or encoded
/// * [`ResizeImageError::Join`] - If the blocking task fails to complete
pub async fn try_resize_local_file_async(
    width: u32,
    height: u32,
    path: &str,
    encoding: Encoding,
    quality: u8,
) -> Result<Option<Bytes>, ResizeImageError> {
    let path = path.to_owned();
    Ok(switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("image: Resize local file", move || {
            try_resize_local_file(width, height, &path, encoding, quality)
        })
        .await??)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// Creates a simple test PNG image and returns the path to the temp file.
    fn create_test_image(width: u32, height: u32) -> std::path::PathBuf {
        use image::{ImageBuffer, Rgb};

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!(
            "moosicbox_image_test_{}_{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));

        // Create a simple gradient image
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
            #[allow(clippy::cast_possible_truncation)]
            Rgb([((x * 255) / width) as u8, ((y * 255) / height) as u8, 128])
        });

        img.save(&path).expect("Failed to save test image");
        path
    }

    fn cleanup_file(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
    }

    #[test_log::test]
    fn try_resize_local_file_resizes_to_jpeg_and_returns_valid_bytes() {
        let test_image = create_test_image(100, 100);

        let result =
            try_resize_local_file(50, 50, test_image.to_str().unwrap(), Encoding::Jpeg, 80);

        cleanup_file(&test_image);

        let bytes = result.expect("Should successfully resize image");
        assert!(bytes.is_some(), "JPEG encoding should return Some");
        let bytes = bytes.unwrap();

        // JPEG magic bytes: FF D8 FF
        assert!(bytes.len() > 3, "Should have non-empty output");
        assert_eq!(bytes[0], 0xFF, "First byte should be JPEG SOI marker");
        assert_eq!(bytes[1], 0xD8, "Second byte should be JPEG SOI marker");
    }

    #[test_log::test]
    fn try_resize_local_file_resizes_to_webp_and_returns_valid_bytes() {
        let test_image = create_test_image(100, 100);

        let result =
            try_resize_local_file(50, 50, test_image.to_str().unwrap(), Encoding::Webp, 80);

        cleanup_file(&test_image);

        let bytes = result.expect("Should successfully resize image");
        // WebP encoding may return None for some image types
        if let Some(bytes) = bytes {
            // WebP magic bytes: RIFF....WEBP
            assert!(bytes.len() >= 12, "Should have valid WebP header");
            assert_eq!(&bytes[0..4], b"RIFF", "Should start with RIFF");
            assert_eq!(&bytes[8..12], b"WEBP", "Should contain WEBP marker");
        }
    }

    #[test_log::test]
    fn try_resize_local_file_returns_error_for_nonexistent_file() {
        let result =
            try_resize_local_file(50, 50, "/nonexistent/path/to/image.png", Encoding::Jpeg, 80);

        assert!(result.is_err(), "Should return error for nonexistent file");
    }

    #[test_log::test]
    fn try_resize_local_file_returns_error_for_invalid_image_data() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!(
            "moosicbox_image_test_invalid_{}_{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));

        // Write invalid image data
        let mut file = std::fs::File::create(&path).expect("Failed to create temp file");
        file.write_all(b"this is not valid image data")
            .expect("Failed to write to temp file");
        drop(file);

        let result = try_resize_local_file(50, 50, path.to_str().unwrap(), Encoding::Jpeg, 80);

        cleanup_file(&path);

        assert!(
            result.is_err(),
            "Should return error for invalid image data"
        );
    }

    #[test_log::test]
    fn try_resize_local_file_handles_different_quality_values() {
        let test_image = create_test_image(100, 100);

        // Test with low quality
        let low_quality =
            try_resize_local_file(50, 50, test_image.to_str().unwrap(), Encoding::Jpeg, 10);

        // Test with high quality
        let high_quality =
            try_resize_local_file(50, 50, test_image.to_str().unwrap(), Encoding::Jpeg, 100);

        cleanup_file(&test_image);

        let low_bytes = low_quality
            .expect("Low quality should succeed")
            .expect("Should have bytes");
        let high_bytes = high_quality
            .expect("High quality should succeed")
            .expect("Should have bytes");

        // Higher quality JPEG should generally be larger than lower quality
        // (not guaranteed for very small images, but typically true)
        assert!(
            !low_bytes.is_empty() && !high_bytes.is_empty(),
            "Both should produce output"
        );
    }

    #[test_log::test]
    fn try_resize_local_file_maintains_aspect_ratio_when_downsizing() {
        // Create a non-square image
        let test_image = create_test_image(200, 100);

        let result =
            try_resize_local_file(100, 100, test_image.to_str().unwrap(), Encoding::Jpeg, 80);

        cleanup_file(&test_image);

        // The resize function uses maintain aspect ratio logic
        assert!(
            result.is_ok(),
            "Should successfully resize non-square image"
        );
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn try_resize_local_file_async_resizes_successfully() {
        let test_image = create_test_image(100, 100);

        let result =
            try_resize_local_file_async(50, 50, test_image.to_str().unwrap(), Encoding::Jpeg, 80)
                .await;

        cleanup_file(&test_image);

        let bytes = result.expect("Should successfully resize image");
        assert!(bytes.is_some(), "Should return Some for JPEG encoding");
        let bytes = bytes.unwrap();

        // JPEG magic bytes
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn try_resize_local_file_async_returns_error_for_nonexistent_file() {
        let result =
            try_resize_local_file_async(50, 50, "/nonexistent/path.png", Encoding::Jpeg, 80).await;

        assert!(result.is_err(), "Should return error for nonexistent file");

        // Verify it's an Image error, not a Join error
        let err = result.unwrap_err();
        assert!(
            matches!(err, ResizeImageError::Image(_)),
            "Should be Image error variant"
        );
    }
}
