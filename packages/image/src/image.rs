use bytes::Bytes;
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType};
use thiserror::Error;

use crate::Encoding;

/// Resizes an image file and encodes it in the specified format.
///
/// # Errors
///
/// * If the image encoder fails to encode the resized image
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
    /// Image processing error.
    #[error(transparent)]
    Image(#[from] image::error::ImageError),
    /// Task join error.
    #[error(transparent)]
    Join(#[from] switchy_async::task::JoinError),
}

/// Asynchronously resizes an image file and encodes it in the specified format.
///
/// # Errors
///
/// * If the image encoder fails to encode the resized image
/// * If the `tokio` task fails to join
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
