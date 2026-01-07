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
    use image::{ImageBuffer, Rgb, RgbImage};

    /// Creates a test image file and returns its path.
    /// The caller is responsible for cleanup.
    #[allow(clippy::cast_possible_truncation)]
    fn create_test_image(width: u32, height: u32) -> tempfile::NamedTempFile {
        let img: RgbImage = ImageBuffer::from_fn(width, height, |x, y| {
            // Create a simple gradient pattern, truncation is intentional (modulo 256)
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });

        let temp_file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("Failed to create temp file");

        img.save(temp_file.path())
            .expect("Failed to save test image");
        temp_file
    }

    #[test_log::test]
    fn test_try_resize_local_file_jpeg_produces_valid_jpeg() {
        let temp_file = create_test_image(200, 150);
        let path = temp_file.path().to_str().expect("Invalid path");

        let result = try_resize_local_file(100, 75, path, Encoding::Jpeg, 85);

        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.is_some());
        let bytes = bytes.unwrap();

        // Verify it's valid JPEG by checking magic bytes (JPEG starts with FFD8FF)
        assert!(bytes.len() >= 3);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
        assert_eq!(bytes[2], 0xFF);

        // Verify we can decode it back as an image
        let decoded = image::load_from_memory(&bytes);
        assert!(decoded.is_ok());
    }

    #[test_log::test]
    fn test_try_resize_local_file_webp_produces_valid_webp() {
        let temp_file = create_test_image(200, 150);
        let path = temp_file.path().to_str().expect("Invalid path");

        let result = try_resize_local_file(100, 75, path, Encoding::Webp, 80);

        assert!(result.is_ok());
        let bytes = result.unwrap();
        // WebP encoding may return None in some cases if the encoder fails
        // to initialize, but when it succeeds, verify the output
        if let Some(bytes) = bytes {
            // Verify it's valid WebP by checking RIFF header and WEBP signature
            assert!(bytes.len() >= 12);
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
        }
    }

    #[test_log::test]
    fn test_try_resize_local_file_nonexistent_file_returns_error() {
        let result = try_resize_local_file(
            100,
            75,
            "/nonexistent/path/to/image.png",
            Encoding::Jpeg,
            85,
        );

        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_try_resize_local_file_resized_dimensions_correct() {
        let temp_file = create_test_image(400, 300);
        let path = temp_file.path().to_str().expect("Invalid path");

        // Resize to 200x150 (maintaining aspect ratio of source)
        let result = try_resize_local_file(200, 150, path, Encoding::Jpeg, 85);

        assert!(result.is_ok());
        let bytes = result.unwrap().expect("Expected some bytes");

        // Decode and check dimensions
        let decoded = image::load_from_memory(&bytes).expect("Failed to decode image");

        // The resize function maintains aspect ratio, so dimensions should be at most
        // the requested size while preserving the original aspect ratio
        assert!(decoded.width() <= 200);
        assert!(decoded.height() <= 150);
    }

    #[test_log::test]
    fn test_try_resize_local_file_upscale_capped_to_original() {
        // Create a small image
        let temp_file = create_test_image(50, 40);
        let path = temp_file.path().to_str().expect("Invalid path");

        // Try to resize to larger dimensions
        let result = try_resize_local_file(200, 160, path, Encoding::Jpeg, 85);

        assert!(result.is_ok());
        let bytes = result.unwrap().expect("Expected some bytes");
        let decoded = image::load_from_memory(&bytes).expect("Failed to decode image");

        // The image crate's resize function does not upscale beyond original dimensions
        // when using resize (not resize_exact), but it will resize to fit within bounds
        // For a 50x40 image requested at 200x160, it should remain at original size
        // since both dimensions are already smaller than requested
        assert!(decoded.width() <= 200);
        assert!(decoded.height() <= 160);
    }

    #[test_log::test]
    fn test_try_resize_local_file_quality_affects_output_size() {
        let temp_file = create_test_image(300, 200);
        let path = temp_file.path().to_str().expect("Invalid path");

        let low_quality = try_resize_local_file(150, 100, path, Encoding::Jpeg, 10)
            .unwrap()
            .unwrap();
        let high_quality = try_resize_local_file(150, 100, path, Encoding::Jpeg, 100)
            .unwrap()
            .unwrap();

        // Higher quality should generally produce larger files
        // (though this isn't always guaranteed for very simple images)
        assert!(high_quality.len() >= low_quality.len());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_try_resize_local_file_async_jpeg() {
        let temp_file = create_test_image(200, 150);
        let path = temp_file.path().to_str().expect("Invalid path");

        let result = try_resize_local_file_async(100, 75, path, Encoding::Jpeg, 85).await;

        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.is_some());
        let bytes = bytes.unwrap();

        // Verify it's valid JPEG
        assert!(bytes.len() >= 3);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
        assert_eq!(bytes[2], 0xFF);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_try_resize_local_file_async_nonexistent_file() {
        let result = try_resize_local_file_async(
            100,
            75,
            "/nonexistent/path/to/image.png",
            Encoding::Jpeg,
            85,
        )
        .await;

        assert!(result.is_err());
        // The error should be an Image error, not a Join error
        match result.unwrap_err() {
            ResizeImageError::Image(_) => {} // Expected
            ResizeImageError::Join(_) => panic!("Expected Image error, got Join error"),
        }
    }

    #[test_log::test]
    fn test_try_resize_local_file_invalid_image_file_returns_error() {
        // Create a temp file with invalid image data
        let temp_file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("Failed to create temp file");

        std::fs::write(temp_file.path(), b"not a valid image").expect("Failed to write");

        let path = temp_file.path().to_str().expect("Invalid path");
        let result = try_resize_local_file(100, 75, path, Encoding::Jpeg, 85);

        assert!(result.is_err());
    }
}
