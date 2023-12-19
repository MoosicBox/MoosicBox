use std::path::PathBuf;

use actix_web::{
    body::SizedStream,
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    http::header::{CacheControl, CacheDirective, ContentType},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use moosicbox_core::{
    app::AppState,
    sqlite::db::get_track,
    track_range::{parse_track_id_ranges, ParseTrackIdsError},
    types::{AudioFormat, PlaybackQuality},
};
use serde::Deserialize;

use crate::files::{
    album::{get_album_cover, AlbumCoverError, AlbumCoverSource},
    artist::{get_artist_cover, ArtistCoverError, ArtistCoverSource},
    track::{
        get_or_init_track_size, get_track_info, get_track_source, get_tracks_info, TrackInfo,
        TrackInfoError, TrackSource, TrackSourceError,
    },
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    pub track_id: i32,
    pub format: Option<AudioFormat>,
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
    let source = get_track_source(
        query.track_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?;

    let format = query.format.unwrap_or_default();

    let size = get_or_init_track_size(
        query.track_id,
        &source,
        PlaybackQuality { format },
        &data
            .db
            .as_ref()
            .ok_or(ErrorInternalServerError("No DB set"))?
            .library
            .lock()
            .unwrap(),
    )?;

    match source {
        TrackSource::LocalFilePath(path) => match format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => Ok(HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, "audio/mp4"))
                .body(SizedStream::new(
                    size,
                    moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac_stream(
                        path,
                    ),
                ))),
            #[cfg(feature = "flac")]
            AudioFormat::Flac => {
                let track = get_track(
                    &data
                        .db
                        .as_ref()
                        .ok_or(ErrorInternalServerError("No DB set"))?
                        .library
                        .lock()
                        .unwrap()
                        .inner,
                    query.track_id,
                )
                .map_err(|e| ErrorInternalServerError(format!("DbError: {}", e)))?
                .ok_or(ErrorNotFound(format!("Missing track {}", query.track_id)))?;

                if track.format != Some(AudioFormat::Flac) {
                    return Err(ErrorBadRequest("Unsupported format FLAC"));
                }

                Ok(
                    actix_files::NamedFile::open_async(PathBuf::from(path).as_path())
                        .await?
                        .into_response(&req),
                )
            }
            #[cfg(feature = "mp3")]
            AudioFormat::Mp3 => Ok(HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, "audio/mp3"))
                .body(SizedStream::new(
                    size,
                    moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3_stream(
                        path,
                    ),
                ))),
            #[cfg(feature = "opus")]
            AudioFormat::Opus => Ok(HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, "audio/opus"))
                .body(SizedStream::new(
                    size,
                    moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus_stream(
                        path,
                    ),
                ))),
            AudioFormat::Source => Ok(actix_files::NamedFile::open_async(
                PathBuf::from(path).as_path(),
            )
            .await?
            .into_response(&req)),
        },
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackInfoQuery {
    pub track_id: usize,
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
            query.track_id as i32,
            data.db
                .clone()
                .ok_or(ErrorInternalServerError("No DB set"))?,
        )
        .await?,
    ))
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTracksInfoQuery {
    pub track_ids: String,
}

#[route("/tracks/info", method = "GET")]
pub async fn tracks_info_endpoint(
    query: web::Query<GetTracksInfoQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<TrackInfo>>> {
    let ids = parse_track_id_ranges(&query.track_ids).map_err(|e| match e {
        ParseTrackIdsError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseTrackIdsError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseTrackIdsError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    Ok(Json(
        get_tracks_info(
            ids,
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
