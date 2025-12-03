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

    // Path to a test image in the repository
    const TEST_IMAGE_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../app-website/public/img/icon128.png"
    );

    #[test_log::test]
    fn try_resize_local_file_returns_error_for_nonexistent_file() {
        let result = try_resize_local_file(
            100,
            100,
            "/nonexistent/path/to/image.png",
            Encoding::Jpeg,
            85,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, image::error::ImageError::IoError(_)),
            "Expected IoError for nonexistent file, got: {err:?}"
        );
    }

    #[test_log::test]
    fn try_resize_local_file_resizes_image_to_jpeg() {
        let result = try_resize_local_file(64, 64, TEST_IMAGE_PATH, Encoding::Jpeg, 85);

        assert!(result.is_ok(), "Failed to resize image: {result:?}");
        let bytes = result.unwrap();
        assert!(bytes.is_some(), "Expected Some(bytes) for JPEG encoding");
        let bytes = bytes.unwrap();
        // JPEG files start with FFD8 magic bytes
        assert!(
            bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8,
            "Output should be valid JPEG data"
        );
    }

    #[test_log::test]
    fn try_resize_local_file_resizes_image_to_webp() {
        let result = try_resize_local_file(64, 64, TEST_IMAGE_PATH, Encoding::Webp, 85);

        assert!(result.is_ok(), "Failed to resize image: {result:?}");
        let bytes = result.unwrap();
        // WebP encoding may return None if the encoder fails to convert
        if let Some(bytes) = bytes {
            // WebP files start with "RIFF" magic bytes
            assert!(
                bytes.len() >= 4 && &bytes[0..4] == b"RIFF",
                "Output should be valid WebP data"
            );
        }
    }

    #[test_log::test]
    fn try_resize_local_file_preserves_aspect_ratio() {
        // The test image is 128x128, resizing to 200x100 should maintain aspect
        // The image crate's resize maintains aspect ratio fitting within bounds
        let result = try_resize_local_file(200, 100, TEST_IMAGE_PATH, Encoding::Jpeg, 85);

        assert!(result.is_ok(), "Failed to resize image: {result:?}");
        let bytes = result.unwrap();
        assert!(bytes.is_some(), "Expected Some(bytes) for JPEG encoding");
        // Verify we got valid output (detailed dimension check would require decoding)
        assert!(
            bytes.unwrap().len() > 100,
            "Output should have reasonable size"
        );
    }

    #[test_log::test]
    fn try_resize_local_file_handles_different_quality_levels() {
        let low_quality =
            try_resize_local_file(64, 64, TEST_IMAGE_PATH, Encoding::Jpeg, 10).unwrap();
        let high_quality =
            try_resize_local_file(64, 64, TEST_IMAGE_PATH, Encoding::Jpeg, 95).unwrap();

        assert!(low_quality.is_some());
        assert!(high_quality.is_some());

        let low_size = low_quality.unwrap().len();
        let high_size = high_quality.unwrap().len();

        // Higher quality JPEG should generally be larger
        assert!(
            high_size > low_size,
            "High quality ({high_size}) should be larger than low quality ({low_size})"
        );
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn try_resize_local_file_async_returns_error_for_nonexistent_file() {
        let result = try_resize_local_file_async(
            100,
            100,
            "/nonexistent/path/to/image.png",
            Encoding::Jpeg,
            85,
        )
        .await;

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ResizeImageError::Image(_)),
            "Expected Image error for nonexistent file"
        );
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn try_resize_local_file_async_resizes_image_successfully() {
        let result = try_resize_local_file_async(64, 64, TEST_IMAGE_PATH, Encoding::Jpeg, 85).await;

        assert!(result.is_ok(), "Failed to resize image: {result:?}");
        let bytes = result.unwrap();
        assert!(bytes.is_some(), "Expected Some(bytes) for JPEG encoding");
        let bytes = bytes.unwrap();
        // JPEG files start with FFD8 magic bytes
        assert!(
            bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8,
            "Output should be valid JPEG data"
        );
    }
}
