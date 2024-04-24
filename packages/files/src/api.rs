use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use futures::StreamExt;
use moosicbox_core::{
    app::AppState,
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::models::{AlbumId, ApiSource, ArtistId, TrackApiSource},
    types::AudioFormat,
};
use serde::Deserialize;

use crate::files::{
    album::{get_album_cover, AlbumCoverError, AlbumCoverSource},
    artist::{get_artist_cover, ArtistCoverError, ArtistCoverSource},
    resize_image_path,
    track::{
        audio_format_to_content_type, get_or_init_track_visualization, get_track_bytes,
        get_track_id_source, get_track_info, get_tracks_info, track_source_to_content_type,
        GetTrackBytesError, TrackAudioQuality, TrackInfo, TrackInfoError, TrackSource,
        TrackSourceError,
    },
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    pub track_id: u64,
    pub format: Option<AudioFormat>,
    pub quality: Option<TrackAudioQuality>,
    pub source: Option<TrackApiSource>,
}

impl From<TrackSourceError> for actix_web::Error {
    fn from(err: TrackSourceError) -> Self {
        log::error!("TrackSourceError {err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<TrackInfoError> for actix_web::Error {
    fn from(err: TrackInfoError) -> Self {
        log::error!("TrackInfoError {err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackVisualizationQuery {
    pub track_id: i32,
    pub max: Option<u16>,
}

#[route("/track/visualization", method = "GET")]
pub async fn track_visualization_endpoint(
    _req: HttpRequest,
    query: web::Query<GetTrackVisualizationQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<u8>>> {
    let source = get_track_id_source(query.track_id, data.database.clone(), None, None).await?;

    Ok(Json(get_or_init_track_visualization(
        query.track_id,
        &source,
        query.max.unwrap_or(333),
    )?))
}

impl From<GetTrackBytesError> for actix_web::Error {
    fn from(err: GetTrackBytesError) -> Self {
        log::error!("GetTrackBytesError {err:?}");
        match err {
            GetTrackBytesError::Db(_) => ErrorInternalServerError(err),
            GetTrackBytesError::IO(_) => ErrorInternalServerError(err),
            GetTrackBytesError::Reqwest(_) => ErrorInternalServerError(err),
            GetTrackBytesError::TrackInfo(_) => ErrorInternalServerError(err),
            GetTrackBytesError::NotFound => ErrorNotFound(err),
            GetTrackBytesError::UnsupportedFormat => ErrorBadRequest(err),
        }
    }
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    #[cfg(feature = "track-range")] req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let source = get_track_id_source(
        query.track_id as i32,
        data.database.clone(),
        query.quality,
        query.source,
    )
    .await?;

    let content_type = query
        .format
        .as_ref()
        .and_then(audio_format_to_content_type)
        .or(track_source_to_content_type(&source));

    let format = query.format.unwrap_or_default();

    #[cfg(feature = "track-range")]
    let range = req
        .headers()
        .get(actix_web::http::header::RANGE)
        .and_then(|x| x.to_str().ok())
        .map(|range| {
            log::debug!("Got range request {:?}", range);

            range
                .strip_prefix("bytes=")
                .map(|s| s.to_string())
                .ok_or(ErrorBadRequest(format!("Invalid range: {range}")))
        })
        .transpose()?
        .map(|range| {
            crate::range::parse_range(&range)
                .map_err(|e| ErrorBadRequest(format!("Invalid bytes range: {range} ({e:?})")))
        })
        .transpose()?;

    #[cfg(not(feature = "track-range"))]
    let range: Option<crate::range::Range> = None;

    let mut response = HttpResponse::Ok();

    if let Some(content_type) = content_type {
        response.insert_header((actix_web::http::header::CONTENT_TYPE, content_type));
    } else {
        match &source {
            TrackSource::Tidal { .. } | TrackSource::Qobuz { .. } => {
                #[cfg(feature = "flac")]
                {
                    response.insert_header((
                        actix_web::http::header::CONTENT_TYPE,
                        audio_format_to_content_type(&AudioFormat::Flac).unwrap(),
                    ));
                }
                #[cfg(not(feature = "flac"))]
                {
                    log::warn!("No valid CONTENT_TYPE available for audio format {format:?}");
                }
            }
            _ => {
                log::warn!("Failed to get CONTENT_TYPE for track source");
            }
        }
    }

    log::debug!("Fetching track bytes with range range={range:?}");

    let bytes = get_track_bytes(
        data.database.clone(),
        query.track_id,
        source,
        format,
        true,
        range.as_ref().and_then(|r| r.start.map(|x| x as u64)),
        range.as_ref().and_then(|r| r.end.map(|x| x as u64)),
    )
    .await?;

    log::debug!("Got bytes with size={:?}", bytes.size);

    let stream = bytes.stream.filter_map(|x| async { x.ok() });

    if let Some(mut size) = bytes.size {
        if let Some(range) = range {
            if let Some(end) = range.end {
                if end > size as usize {
                    let error = format!("Range end out of bounds: {end}");
                    log::error!("{}", error);
                    return Err(ErrorBadRequest(error));
                }
                size = end as u64;
            }
            if let Some(start) = range.start {
                if start > size as usize {
                    let error = format!("Range start out of bounds: {start}");
                    log::error!("{}", error);
                    return Err(ErrorBadRequest(error));
                }
                size -= start as u64;
            }
        }

        log::debug!("Returning stream body with size={:?}", size);
        Ok(response.body(actix_web::body::SizedStream::new(size, stream)))
    } else {
        Ok(response.streaming(stream))
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackInfoQuery {
    pub track_id: usize,
}

#[route("/track/info", method = "GET")]
pub async fn track_info_endpoint(
    query: web::Query<GetTrackInfoQuery>,
    data: web::Data<AppState>,
) -> Result<Json<TrackInfo>> {
    Ok(Json(
        get_track_info(query.track_id as u64, &**data.database).await?,
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
    let ids = parse_integer_ranges(&query.track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    Ok(Json(get_tracks_info(ids, &**data.database).await?))
}

impl From<ArtistCoverError> for actix_web::Error {
    fn from(err: ArtistCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCoverQuery {
    pub source: Option<ApiSource>,
}

#[route("/artists/{artist_id}/source", method = "GET", method = "HEAD")]
pub async fn artist_source_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<ArtistCoverQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let artist_id_string = paths;
    let artist_id = match query.source.unwrap_or(ApiSource::Library) {
        ApiSource::Library => artist_id_string.parse::<i32>().map(ArtistId::Library),
        ApiSource::Tidal => artist_id_string.parse::<u64>().map(ArtistId::Tidal),
        ApiSource::Qobuz => artist_id_string.parse::<u64>().map(ArtistId::Qobuz),
    }
    .map_err(|_e| ErrorBadRequest("Invalid artist_id"))?;

    match get_artist_cover(artist_id, data.database.clone(), None).await? {
        ArtistCoverSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);
            let file_path = path_buf.as_path();

            let file = actix_files::NamedFile::open_async(file_path)
                .await
                .map_err(|e| {
                    ArtistCoverError::File(file_path.to_str().unwrap().into(), format!("{e:?}"))
                })
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

            Ok(file.into_response(&req))
        }
    }
}

#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    path: web::Path<(String, String)>,
    query: web::Query<ArtistCoverQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let artist_id_string = paths.0;
    let artist_id = match query.source.unwrap_or(ApiSource::Library) {
        ApiSource::Library => artist_id_string.parse::<i32>().map(ArtistId::Library),
        ApiSource::Tidal => artist_id_string.parse::<u64>().map(ArtistId::Tidal),
        ApiSource::Qobuz => artist_id_string.parse::<u64>().map(ArtistId::Qobuz),
    }
    .map_err(|_e| ErrorBadRequest("Invalid artist_id"))?;

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

    let ArtistCoverSource::LocalFilePath(path) = get_artist_cover(
        artist_id,
        data.database.clone(),
        Some(std::cmp::max(width, height)),
    )
    .await?;

    resize_image_path(&path, width, height)
        .map_err(|e| ErrorInternalServerError(format!("Failed to resize image: {e:?}")))
}

impl From<AlbumCoverError> for actix_web::Error {
    fn from(err: AlbumCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AlbumCoverQuery {
    pub source: Option<ApiSource>,
}

#[route("/albums/{album_id}/source", method = "GET", method = "HEAD")]
pub async fn album_source_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<AlbumCoverQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let album_id_string = paths;
    let album_id = match query.source.unwrap_or(ApiSource::Library) {
        ApiSource::Library => album_id_string.parse::<i32>().map(AlbumId::Library),
        ApiSource::Tidal => album_id_string.parse::<u64>().map(AlbumId::Tidal),
        ApiSource::Qobuz => Ok(AlbumId::Qobuz(album_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;

    match get_album_cover(album_id, data.database.clone(), None).await? {
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
    query: web::Query<AlbumCoverQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let album_id_string = paths.0;
    let album_id = match query.source.unwrap_or(ApiSource::Library) {
        ApiSource::Library => album_id_string.parse::<i32>().map(AlbumId::Library),
        ApiSource::Tidal => album_id_string.parse::<u64>().map(AlbumId::Tidal),
        ApiSource::Qobuz => Ok(AlbumId::Qobuz(album_id_string)),
    }
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

    let AlbumCoverSource::LocalFilePath(path) = get_album_cover(
        album_id,
        data.database.clone(),
        Some(std::cmp::max(width, height)),
    )
    .await?;

    resize_image_path(&path, width, height)
        .map_err(|e| ErrorInternalServerError(format!("Failed to resize image: {e:?}")))
}
