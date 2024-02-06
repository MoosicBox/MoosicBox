use actix_web::{
    http::header::{CacheControl, CacheDirective},
    HttpResponse,
};
use thiserror::Error;

pub mod album;
pub mod artist;
pub mod track;

#[derive(Debug, Error)]
pub enum ResizeImageError {
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
    #[error("No image resize features enabled")]
    NoImageResizeFeaturesEnabled,
}

#[allow(unused)]
pub(crate) fn resize_image_path(
    path: &str,
    width: u32,
    height: u32,
) -> Result<HttpResponse, ResizeImageError> {
    let mut response = HttpResponse::Ok();

    response.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32 * 14)]));

    #[cfg(feature = "libvips")]
    let resized = {
        use log::error;
        use moosicbox_image::libvips::{get_error, resize_local_file};
        response.content_type(actix_web::http::header::ContentType::jpeg());
        let resized = resize_local_file(width, height, &path).map_err(|e| {
            error!("{}", get_error());
            ResizeImageError::File(path, e.to_string())
        })?;

        return Ok(response.body(resized));
    };
    #[cfg(feature = "image")]
    {
        use moosicbox_image::{image::try_resize_local_file, Encoding};
        let resized = if let Ok(Some(resized)) =
            try_resize_local_file(width, height, path, Encoding::Webp, 80)
                .map_err(|e| ResizeImageError::File(path.to_string(), e.to_string()))
        {
            response.content_type("image/webp");
            resized
        } else {
            response.content_type(actix_web::http::header::ContentType::jpeg());
            try_resize_local_file(width, height, path, Encoding::Jpeg, 80)
                .map_err(|e| ResizeImageError::File(path.to_string(), e.to_string()))?
                .expect("Failed to resize to jpeg image")
        };

        return Ok(response.body(resized));
    }

    #[allow(unreachable_code)]
    Err(ResizeImageError::NoImageResizeFeaturesEnabled)
}
