use std::path::PathBuf;

use actix_web::{
    body::SizedStream,
    error::{ErrorBadRequest, ErrorInternalServerError},
    http::header::{CacheControl, CacheDirective},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use lazy_static::lazy_static;
use moosicbox_core::{
    app::AppState,
    track_range::{parse_track_id_ranges, ParseTrackIdsError},
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_stream_utils::ByteWriter;
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStream, output::AudioOutputHandler,
    play_file_path_str, play_media_source,
};
use serde::Deserialize;
use symphonia::core::{io::MediaSourceStream, probe::Hint};
use tokio_util::sync::CancellationToken;

use crate::files::{
    album::{get_album_cover, AlbumCoverError, AlbumCoverSource},
    artist::{get_artist_cover, ArtistCoverError, ArtistCoverSource},
    track::{
        get_or_init_track_size, get_or_init_track_visualization, get_track_info, get_track_source,
        get_tracks_info, TrackInfo, TrackInfoError, TrackSource, TrackSourceError,
    },
};

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

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

impl From<TrackInfoError> for actix_web::Error {
    fn from(err: TrackInfoError) -> Self {
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
    let source = get_track_source(
        query.track_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?;

    Ok(Json(get_or_init_track_visualization(
        query.track_id,
        &source,
        query.max.unwrap_or(333),
    )?))
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

    let writer = ByteWriter::default();
    let stream = writer.stream();

    {
        let source = source.clone();
        RT.spawn(async move {
            let audio_output_handler = match format {
                #[cfg(feature = "aac")]
                AudioFormat::Aac => {
                    use moosicbox_symphonia_player::output::encoder::aac::encoder::AacEncoder;
                    let mut audio_output_handler = AudioOutputHandler::new();
                    audio_output_handler.with_output(Box::new(move |spec, duration| {
                        let mut encoder = AacEncoder::new(writer.clone());
                        encoder.open(spec, duration);
                        Ok(Box::new(encoder))
                    }));
                    Some(audio_output_handler)
                }
                #[cfg(feature = "flac")]
                AudioFormat::Flac => None,
                #[cfg(feature = "mp3")]
                AudioFormat::Mp3 => {
                    use moosicbox_symphonia_player::output::encoder::mp3::encoder::Mp3Encoder;
                    let encoder_writer = writer.clone();
                    let mut audio_output_handler = AudioOutputHandler::new();
                    audio_output_handler.with_output(Box::new(move |spec, duration| {
                        let mut encoder = Mp3Encoder::new(encoder_writer.clone());
                        encoder.open(spec, duration);
                        Ok(Box::new(encoder))
                    }));
                    Some(audio_output_handler)
                }
                #[cfg(feature = "opus")]
                AudioFormat::Opus => {
                    use moosicbox_symphonia_player::output::encoder::opus::encoder::OpusEncoder;
                    let encoder_writer = writer.clone();
                    let mut audio_output_handler = AudioOutputHandler::new();
                    audio_output_handler.with_output(Box::new(move |spec, duration| {
                        let mut encoder: OpusEncoder<i16, ByteWriter> =
                            OpusEncoder::new(encoder_writer.clone());
                        encoder.open(spec, duration);
                        Ok(Box::new(encoder))
                    }));
                    Some(audio_output_handler)
                }
                AudioFormat::Source => None,
            };

            if let Some(mut audio_output_handler) = audio_output_handler {
                match source {
                    TrackSource::LocalFilePath(ref path) => {
                        if let Err(err) = play_file_path_str(
                            path,
                            &mut audio_output_handler,
                            true,
                            true,
                            None,
                            None,
                        ) {
                            log::error!("Failed to encode to aac: {err:?}");
                        }
                    }
                    TrackSource::Tidal(url) | TrackSource::Qobuz(url) => {
                        let source = Box::new(RemoteByteStream::new(
                            url,
                            Some(size),
                            true,
                            CancellationToken::new(),
                        ));
                        if let Err(err) = play_media_source(
                            MediaSourceStream::new(source, Default::default()),
                            &Hint::new(),
                            &mut audio_output_handler,
                            true,
                            true,
                            None,
                            None,
                        ) {
                            log::error!("Failed to encode to aac: {err:?}");
                        }
                    }
                }
            }
        });
    }

    match source {
        TrackSource::LocalFilePath(path) => match format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => Ok(HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, "audio/mp4"))
                .body(SizedStream::new(size, stream))),
            #[cfg(feature = "flac")]
            AudioFormat::Flac => {
                let track = moosicbox_core::sqlite::db::get_track(
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
                .ok_or(actix_web::error::ErrorNotFound(format!(
                    "Missing track {}",
                    query.track_id
                )))?;

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
                .body(SizedStream::new(size, stream))),
            #[cfg(feature = "opus")]
            AudioFormat::Opus => Ok(HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, "audio/opus"))
                .body(SizedStream::new(size, stream))),
            AudioFormat::Source => Ok(actix_files::NamedFile::open_async(
                PathBuf::from(path).as_path(),
            )
            .await?
            .into_response(&req)),
        },
        TrackSource::Tidal(url) | TrackSource::Qobuz(url) => {
            let client = reqwest::Client::new();
            let bytes = client.get(url).send().await.unwrap().bytes_stream();
            Ok(HttpResponse::Ok().streaming(bytes))
        }
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

            response.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32 * 14)]));

            #[cfg(feature = "libvips")]
            let resized = {
                use log::error;
                use moosicbox_image::libvips::{get_error, resize_local_file};
                response.content_type(actix_web::http::header::ContentType::jpeg());
                let resized = resize_local_file(width, height, &path).map_err(|e| {
                    error!("{}", get_error());
                    AlbumCoverError::File(path, e.to_string())
                })?;

                return Ok(response.body(resized));
            };
            #[cfg(feature = "image")]
            {
                use moosicbox_image::{image::try_resize_local_file, Encoding};
                let resized = if let Some(resized) =
                    try_resize_local_file(width, height, &path, Encoding::Webp, 80)
                        .map_err(|e| AlbumCoverError::File(path.clone(), e.to_string()))?
                {
                    response.content_type("image/webp");
                    resized
                } else {
                    response.content_type(actix_web::http::header::ContentType::jpeg());
                    try_resize_local_file(width, height, &path, Encoding::Jpeg, 80)
                        .map_err(|e| AlbumCoverError::File(path, e.to_string()))?
                        .expect("Failed to resize to jpeg image")
                };

                return Ok(response.body(resized));
            }

            #[allow(unreachable_code)]
            Err(ErrorInternalServerError(format!(
                "No image resizing features enabled for image '{path}' with size {width}x{height}"
            )))
        }
    }
}
