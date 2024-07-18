use std::str::FromStr as _;

use actix_web::HttpResponse;
use thiserror::Error;

pub mod album;
pub mod artist;
pub mod track;

mod track_bytes_media_source;
pub mod track_pool;

#[derive(Debug, Error)]
pub enum ResizeImageError {
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
    #[error("No image resize features enabled")]
    NoImageResizeFeaturesEnabled,
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

pub(crate) fn filename_from_path_str(path: &str) -> Option<String> {
    std::path::PathBuf::from_str(path).ok().and_then(|p| {
        p.file_name()
            .and_then(|x| x.to_str().map(|x| x.to_string()))
    })
}

#[allow(unused)]
pub(crate) async fn resize_image_path(
    path: &str,
    width: u32,
    height: u32,
) -> Result<HttpResponse, ResizeImageError> {
    #[allow(unused_mut)]
    let mut image_type = "webp";

    #[cfg(feature = "libvips")]
    let resized = {
        use log::error;
        use moosicbox_image::libvips::{get_error, resize_local_file};
        image_type = "jpeg";
        let resized = moosicbox_task::spawn_blocking("files: resize_image_path", {
            let path = path.to_owned();
            move || {
                Ok::<_, ResizeImageError>(resize_local_file(width, height, &path).map_err(|e| {
                    error!("{}", get_error());
                    ResizeImageError::File(path, e.to_string())
                })?)
            }
        })
        .await??;

        let mut response = HttpResponse::Ok();
        response.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32 * 14)]));
        response.content_type(format!("image/{image_type}"));
        return Ok(response.body(resized));
    };
    #[cfg(feature = "image")]
    {
        use moosicbox_image::{image::try_resize_local_file_async, Encoding};
        let resized = if let Ok(Some(resized)) =
            try_resize_local_file_async(width, height, path, Encoding::Webp, 80)
                .await
                .map_err(|e| ResizeImageError::File(path.to_string(), e.to_string()))
        {
            resized
        } else {
            image_type = "jpeg";
            try_resize_local_file_async(width, height, path, Encoding::Jpeg, 80)
                .await
                .map_err(|e| ResizeImageError::File(path.to_string(), e.to_string()))?
                .expect("Failed to resize to jpeg image")
        };

        use actix_web::http::header::{CacheControl, CacheDirective};
        let mut response = HttpResponse::Ok();
        response.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32 * 14)]));
        response.content_type(format!("image/{image_type}"));
        return Ok(response.body(resized));
    }

    #[allow(unreachable_code)]
    Err(ResizeImageError::NoImageResizeFeaturesEnabled)
}
