use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    http::header::{CacheControl, CacheDirective, ContentType},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;

use crate::files::{
    album::{get_album_cover, AlbumCoverError, AlbumCoverSource},
    artist::{get_artist_cover, ArtistCoverError, ArtistCoverSource},
    track::{
        get_track_info, get_track_source, TrackInfo, TrackInfoError, TrackSource, TrackSourceError,
    },
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    pub track_id: i32,
}

impl From<TrackSourceError> for actix_web::Error {
    fn from(err: TrackSourceError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    match get_track_source(
        query.track_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?
    {
        TrackSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);

            Ok(actix_files::NamedFile::open_async(path_buf.as_path())
                .await?
                .into_response(&req))
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackInfoQuery {
    pub track_id: i32,
}

impl From<TrackInfoError> for actix_web::Error {
    fn from(err: TrackInfoError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[route("/track/info", method = "GET")]
pub async fn track_info_endpoint(
    query: web::Query<GetTrackInfoQuery>,
    data: web::Data<AppState>,
) -> Result<Json<TrackInfo>> {
    Ok(Json(
        get_track_info(
            query.track_id,
            data.db
                .clone()
                .ok_or(ErrorInternalServerError("No DB set"))?,
        )
        .await?,
    ))
}

impl From<ArtistCoverError> for actix_web::Error {
    fn from(err: ArtistCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let artist_id = paths
        .0
        .parse::<i32>()
        .map_err(|_e| ErrorInternalServerError("Invalid artist_id"))?;

    match get_artist_cover(
        artist_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?
    {
        ArtistCoverSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);

            Ok(actix_files::NamedFile::open_async(path_buf.as_path())
                .await?
                .into_response(&req))
        }
    }
}

impl From<AlbumCoverError> for actix_web::Error {
    fn from(err: AlbumCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[route("/albums/{album_id}/source", method = "GET", method = "HEAD")]
pub async fn album_source_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let album_id = paths
        .parse::<i32>()
        .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;

    match get_album_cover(
        album_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?
    {
        AlbumCoverSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);
            let file_path = path_buf.as_path();

            let file = actix_files::NamedFile::open_async(file_path)
                .await
                .map_err(|e| {
                    AlbumCoverError::File(file_path.to_str().unwrap().into(), format!("{e:?}"))
                })
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

            Ok(file.into_response(&req))
        }
    }
}

#[route("/albums/{album_id}/{size}", method = "GET", method = "HEAD")]
pub async fn album_artwork_endpoint(
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let album_id = paths
        .0
        .parse::<i32>()
        .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;
    let dimensions = paths
        .1
        .split('x')
        .take(2)
        .map(|dimension| {
            dimension
                .parse::<u32>()
                .map_err(|_e| ErrorBadRequest("Invalid dimension"))
        })
        .collect::<Result<Vec<_>>>()?;
    let (width, height) = (dimensions[0], dimensions[1]);

    match get_album_cover(
        album_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?
    {
        AlbumCoverSource::LocalFilePath(path) => {
            let mut response = HttpResponse::Ok();

            #[cfg(feature = "libvips")]
            let resized = {
                use log::error;
                use moosicbox_image::libvips::{get_error, resize_local_file};
                response.content_type(ContentType::jpeg());
                resize_local_file(width, height, &path).map_err(|e| {
                    error!("{}", get_error());
                    AlbumCoverError::File(path, e.to_string())
                })?
            };
            #[cfg(feature = "image")]
            let resized = {
                use moosicbox_image::{image::try_resize_local_file, Encoding};
                if let Some(resized) =
                    try_resize_local_file(width, height, &path, Encoding::Webp, 80)
                        .map_err(|e| AlbumCoverError::File(path.clone(), e.to_string()))?
                {
                    response.content_type("image/webp");
                    resized
                } else {
                    response.content_type(ContentType::jpeg());
                    try_resize_local_file(width, height, &path, Encoding::Jpeg, 80)
                        .map_err(|e| AlbumCoverError::File(path, e.to_string()))?
                        .expect("Failed to resize to jpeg image")
                }
            };

            response.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32 * 14)]));
            Ok(response.body(resized))
        }
    }
}
