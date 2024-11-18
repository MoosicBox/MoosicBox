use std::sync::LazyLock;

use bytes::Bytes;
use libvips::{ops, VipsApp, VipsImage};
use log::debug;

static VIPS: LazyLock<VipsApp> = LazyLock::new(|| {
    debug!("Initializing libvips");
    let app = VipsApp::new("Moosicbox Libvips", false).expect("Cannot initialize libvips");

    app.concurrency_set(4);

    app
});

pub fn get_error() -> String {
    let error = VIPS.error_buffer().unwrap_or_default().to_string();
    VIPS.error_clear();
    error
}

/// # Errors
///
/// * If the libvips image encoder fails to encode the resized image
pub fn resize_local_file(
    width: u32,
    height: u32,
    path: &str,
) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailImageOptions {
        #[allow(clippy::cast_possible_wrap)]
        height: height as i32,
        import_profile: "sRGB".into(),
        export_profile: "sRGB".into(),
        ..ops::ThumbnailImageOptions::default()
    };

    let image = VipsImage::new_from_file(path)?;

    #[allow(clippy::cast_possible_wrap)]
    let thumbnail = ops::thumbnail_image_with_opts(&image, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}

/// # Errors
///
/// * If the libvips image encoder fails to encode the resized image
pub fn resize_bytes(width: u32, height: u32, bytes: &[u8]) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailBufferOptions {
        #[allow(clippy::cast_possible_wrap)]
        height: height as i32,
        import_profile: "sRGB".into(),
        export_profile: "sRGB".into(),
        ..ops::ThumbnailBufferOptions::default()
    };

    #[allow(clippy::cast_possible_wrap)]
    let thumbnail = ops::thumbnail_buffer_with_opts(bytes, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}
