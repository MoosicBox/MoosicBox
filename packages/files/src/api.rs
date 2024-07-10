use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use futures::StreamExt;
use moosicbox_core::{
    app::AppState,
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::models::{AlbumId, ApiSource, ArtistId},
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

impl From<TrackSourceError> for actix_web::Error {
    fn from(e: TrackSourceError) -> Self {
        match e {
            TrackSourceError::NotFound(e) => ErrorNotFound(e.to_string()),
            TrackSourceError::InvalidSource => ErrorBadRequest(e.to_string()),
            TrackSourceError::Db(_) => ErrorInternalServerError(e.to_string()),
            TrackSourceError::TidalTrackUrl(e) => match e {
                moosicbox_tidal::TidalTrackFileUrlError::AuthenticatedRequest(e) => {
                    ErrorUnauthorized(e.to_string())
                }
                moosicbox_tidal::TidalTrackFileUrlError::Parse(_) => {
                    ErrorInternalServerError(e.to_string())
                }
            },
            TrackSourceError::QobuzTrackUrl(e) => match e {
                moosicbox_qobuz::QobuzTrackFileUrlError::AuthenticatedRequest(_) => {
                    ErrorUnauthorized(e.to_string())
                }
                moosicbox_qobuz::QobuzTrackFileUrlError::Database(_) => {
                    ErrorInternalServerError(e.to_string())
                }
                moosicbox_qobuz::QobuzTrackFileUrlError::Db(_) => {
                    ErrorInternalServerError(e.to_string())
                }
                moosicbox_qobuz::QobuzTrackFileUrlError::NoAppSecretAvailable => {
                    ErrorInternalServerError(e.to_string())
                }
                moosicbox_qobuz::QobuzTrackFileUrlError::Parse(_) => {
                    ErrorInternalServerError(e.to_string())
                }
            },
            TrackSourceError::YtTrackUrl(e) => match e {
                moosicbox_yt::YtTrackFileUrlError::AuthenticatedRequest(e) => {
                    ErrorUnauthorized(e.to_string())
                }
                moosicbox_yt::YtTrackFileUrlError::Parse(_) => {
                    ErrorInternalServerError(e.to_string())
                }
            },
        }
    }
}

impl From<TrackInfoError> for actix_web::Error {
    fn from(e: TrackInfoError) -> Self {
        ErrorInternalServerError(e.to_string())
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackVisualizationQuery {
    pub track_id: i32,
    pub max: Option<u16>,
    pub source: Option<ApiSource>,
}

#[route("/track/visualization", method = "GET")]
pub async fn track_visualization_endpoint(
    query: web::Query<GetTrackVisualizationQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<u8>>> {
    let source = get_track_id_source(
        query.track_id,
        &**data.database,
        None,
        query.source.unwrap_or(ApiSource::Library),
    )
    .await?;

    Ok(Json(
        get_or_init_track_visualization(&source, query.max.unwrap_or(333)).await?,
    ))
}

impl From<GetTrackBytesError> for actix_web::Error {
    fn from(err: GetTrackBytesError) -> Self {
        match err {
            GetTrackBytesError::Db(_)
            | GetTrackBytesError::IO(_)
            | GetTrackBytesError::Reqwest(_)
            | GetTrackBytesError::Acquire(_)
            | GetTrackBytesError::Join(_)
            | GetTrackBytesError::ToStr(_)
            | GetTrackBytesError::ParseInt(_)
            | GetTrackBytesError::Recv(_)
            | GetTrackBytesError::Commander(_)
            | GetTrackBytesError::TrackInfo(_) => ErrorInternalServerError(err),
            GetTrackBytesError::NotFound => ErrorNotFound(err),
            GetTrackBytesError::UnsupportedFormat => ErrorBadRequest(err),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    pub track_id: u64,
    pub format: Option<AudioFormat>,
    pub quality: Option<TrackAudioQuality>,
    pub source: Option<ApiSource>,
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let method = req.method();

    let source = get_track_id_source(
        query.track_id as i32,
        &**data.database,
        query.quality,
        query.source.unwrap_or(ApiSource::Library),
    )
    .await?;

    log::debug!(
        "{method} /track track_id={} quality={:?} query.source={:?} source={source:?}",
        query.track_id,
        query.quality,
        query.source
    );

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
    response.insert_header((actix_web::http::header::ACCEPT_RANGES, "bytes"));
    response.insert_header((actix_web::http::header::CONTENT_ENCODING, "identity"));

    if let Some(content_type) = content_type {
        response.insert_header((actix_web::http::header::CONTENT_TYPE, content_type));
    } else {
        match &source {
            TrackSource::Tidal { .. } | TrackSource::Qobuz { .. } | TrackSource::Yt { .. } => {
                #[cfg(feature = "flac")]
                {
                    response.insert_header((
                        actix_web::http::header::CONTENT_TYPE,
                        audio_format_to_content_type(&AudioFormat::Flac).unwrap(),
                    ));
                }
                #[cfg(not(feature = "flac"))]
                {
                    moosicbox_assert::die_or_warn!(
                        "No valid CONTENT_TYPE available for audio format {format:?}"
                    );
                }
            }
            _ => {
                moosicbox_assert::die_or_warn!("Failed to get CONTENT_TYPE for track source");
            }
        }
    }

    log::debug!("{method} /track Fetching track bytes with range range={range:?}");

    let bytes = get_track_bytes(
        &**data.database,
        query.track_id,
        source,
        format,
        true,
        range.as_ref().and_then(|r| r.start.map(|x| x as u64)),
        range.as_ref().and_then(|r| r.end.map(|x| x as u64)),
    )
    .await?;

    response.insert_header((
        actix_web::http::header::CONTENT_DISPOSITION,
        format!(
            "inline{filename}",
            filename = bytes
                .filename
                .map_or("".into(), |x| format!("; filename=\"{x}\""))
        ),
    ));

    log::debug!(
        "Got bytes with size={:?} original_size={:?}",
        bytes.size,
        bytes.original_size
    );

    let stream = bytes.stream.filter_map(|x| async { x.ok() });

    if let Some(original_size) = bytes.original_size {
        let size = if let Some(range) = range {
            let mut size = original_size;
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

            response.insert_header((
                actix_web::http::header::CONTENT_RANGE,
                format!(
                    "bytes {start}-{end}/{original_size}",
                    start = range.start.map_or("".to_string(), |x| x.to_string()),
                    end = range.end.map(|x| x as u64).unwrap_or(original_size - 1),
                ),
            ));
            size
        } else {
            response.insert_header((
                actix_web::http::header::CONTENT_RANGE,
                format!("bytes -{end}/{original_size}", end = original_size - 1),
            ));
            original_size
        };

        if size != original_size {
            response.status(actix_web::http::StatusCode::PARTIAL_CONTENT);
        }

        log::debug!("Returning stream body with size={:?}", size);
        Ok(response.body(actix_web::body::SizedStream::new(size, stream)))
    } else {
        log::debug!("No size was found for stream");
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
        ApiSource::Yt => artist_id_string.parse::<u64>().map(ArtistId::Yt),
    }
    .map_err(|_e| ErrorBadRequest("Invalid artist_id"))?;

    match get_artist_cover(artist_id, &**data.database, None).await? {
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
        ApiSource::Yt => artist_id_string.parse::<u64>().map(ArtistId::Yt),
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
        &**data.database,
        Some(std::cmp::max(width, height)),
    )
    .await?;

    resize_image_path(&path, width, height)
        .await
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
        ApiSource::Yt => Ok(AlbumId::Yt(album_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;

    match get_album_cover(album_id, &**data.database, None).await? {
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
        ApiSource::Yt => Ok(AlbumId::Yt(album_id_string)),
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
        &**data.database,
        Some(std::cmp::max(width, height)),
    )
    .await?;

    resize_image_path(&path, width, height)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to resize image: {e:?}")))
}
