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
