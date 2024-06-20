use std::{
    env,
    fs::File,
    pin::Pin,
    str::FromStr,
    sync::{Arc, RwLock},
};

use bytes::{Bytes, BytesMut};
use futures::{prelude::*, StreamExt};
use futures_core::Stream;
use lazy_static::lazy_static;
use moosicbox_core::{
    sqlite::{
        db::{get_track, get_track_size, get_tracks, set_track_size, DbError, SetTrackSize},
        models::{LibraryTrack, TrackApiSource},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::{Database, DatabaseValue};
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use moosicbox_qobuz::{QobuzAudioQuality, QobuzTrackFileUrlError};
use moosicbox_stream_utils::{
    new_byte_writer_id, remote_bytestream::RemoteByteStream, stalled_monitor::StalledReadMonitor,
    ByteWriter,
};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStreamMediaSource, output::AudioOutputHandler,
    play_file_path_str, play_media_source, PlaybackError,
};
use moosicbox_tidal::{TidalAudioQuality, TidalTrackFileUrlError};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use symphonia::core::{
    audio::{AudioBuffer, Signal},
    conv::IntoSample,
    io::MediaSourceStream,
    probe::Hint,
    sample::Sample,
    util::clamp::clamp_i16,
};
use thiserror::Error;
use tokio::io::AsyncSeekExt;
use tokio_util::{
    codec::{BytesCodec, FramedRead},
    sync::CancellationToken,
};

use crate::files::{
    filename_from_path_str, track_bytes_media_source::TrackBytesMediaSource,
    track_pool::get_or_fetch_track,
};

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

#[derive(Clone, Debug)]
pub enum TrackSource {
    LocalFilePath {
        path: String,
        format: AudioFormat,
        track_id: Option<u64>,
    },
    Tidal {
        url: String,
        format: AudioFormat,
        track_id: Option<u64>,
    },
    Qobuz {
        url: String,
        format: AudioFormat,
        track_id: Option<u64>,
    },
}

impl TrackSource {
    pub fn format(&self) -> AudioFormat {
        match self {
            TrackSource::LocalFilePath { format, .. } => *format,
            TrackSource::Tidal { format, .. } => *format,
            TrackSource::Qobuz { format, .. } => *format,
        }
    }

    pub fn track_id(&self) -> Option<u64> {
        match self {
            TrackSource::LocalFilePath { track_id, .. } => *track_id,
            TrackSource::Tidal { track_id, .. } => *track_id,
            TrackSource::Qobuz { track_id, .. } => *track_id,
        }
    }
}

pub fn track_source_to_content_type(source: &TrackSource) -> Option<String> {
    audio_format_to_content_type(&source.format())
}

pub fn audio_format_to_content_type(format: &AudioFormat) -> Option<String> {
    match format {
        #[cfg(feature = "aac")]
        AudioFormat::Aac => Some("audio/m4a".into()),
        #[cfg(feature = "flac")]
        AudioFormat::Flac => Some("audio/flac".into()),
        #[cfg(feature = "mp3")]
        AudioFormat::Mp3 => Some("audio/mp3".into()),
        #[cfg(feature = "opus")]
        AudioFormat::Opus => Some("audio/opus".into()),
        AudioFormat::Source => None,
    }
}

#[derive(Debug, Default, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackAudioQuality {
    Low,          // MP3 320
    FlacLossless, // FLAC 16 bit 44.1kHz
    FlacHiRes,    // FLAC 24 bit <= 96kHz
    #[default]
    FlacHighestRes, // FLAC 24 bit > 96kHz <= 192kHz
}

impl MissingValue<TrackAudioQuality> for &moosicbox_database::Row {}
impl ToValueType<TrackAudioQuality> for DatabaseValue {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

impl ToValueType<TrackAudioQuality> for &serde_json::Value {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

impl MissingValue<TrackAudioQuality> for &rusqlite::Row<'_> {}
impl ToValueType<TrackAudioQuality> for rusqlite::types::Value {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        match self {
            rusqlite::types::Value::Text(str) => Ok(TrackAudioQuality::from_str(&str)
                .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))?),
            _ => Err(ParseError::ConvertType("TrackAudioQuality".into())),
        }
    }
}

impl From<TrackAudioQuality> for TidalAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => TidalAudioQuality::High,
            TrackAudioQuality::FlacLossless => TidalAudioQuality::Lossless,
            TrackAudioQuality::FlacHiRes => TidalAudioQuality::HiResLossless,
            TrackAudioQuality::FlacHighestRes => TidalAudioQuality::HiResLossless,
        }
    }
}

impl From<TrackAudioQuality> for QobuzAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => QobuzAudioQuality::Low,
            TrackAudioQuality::FlacLossless => QobuzAudioQuality::FlacLossless,
            TrackAudioQuality::FlacHiRes => QobuzAudioQuality::FlacHiRes,
            TrackAudioQuality::FlacHighestRes => QobuzAudioQuality::FlacHighestRes,
        }
    }
}

#[derive(Debug, Error)]
pub enum TrackSourceError {
    #[error("Track not found: {0}")]
    NotFound(i32),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TidalTrackUrl(#[from] TidalTrackFileUrlError),
    #[error(transparent)]
    QobuzTrackUrl(#[from] QobuzTrackFileUrlError),
}

pub async fn get_track_id_source(
    track_id: i32,
    db: &dyn Database,
    quality: Option<TrackAudioQuality>,
    source: TrackApiSource,
) -> Result<TrackSource, TrackSourceError> {
    log::debug!("get_track_id_source: track_id={track_id} quality={quality:?} source={source:?}",);

    match source {
        TrackApiSource::Local => {
            let track = get_track(db, track_id as u64)
                .await?
                .ok_or(TrackSourceError::NotFound(track_id))?;

            get_track_source(track_id, Some(track).as_ref(), db, quality, source).await
        }
        _ => get_track_source(track_id, None, db, quality, source).await,
    }
}

pub async fn get_track_source(
    track_id: i32,
    track: Option<&LibraryTrack>,
    db: &dyn Database,
    quality: Option<TrackAudioQuality>,
    source: TrackApiSource,
) -> Result<TrackSource, TrackSourceError> {
    log::debug!(
        "get_track_source: track_id={:?} quality={quality:?} source={source:?}",
        track.map(|x| x.id),
    );

    log::debug!("Got track {track:?}. Getting source={source:?}");

    match source {
        TrackApiSource::Local => {
            let track = track.expect("Missing track");
            match &track.file {
                Some(file) => match env::consts::OS {
                    "windows" => Ok(TrackSource::LocalFilePath {
                        path: Regex::new(r"/mnt/(\w+)")
                            .unwrap()
                            .replace(file, |caps: &Captures| {
                                format!("{}:", caps[1].to_uppercase())
                            })
                            .replace('/', "\\"),
                        format: track.format.unwrap_or(AudioFormat::Source),
                        track_id: Some(track.id.try_into().expect("Invalid track id")),
                    }),
                    _ => Ok(TrackSource::LocalFilePath {
                        path: file.to_string(),
                        format: track.format.unwrap_or(AudioFormat::Source),
                        track_id: Some(track.id.try_into().expect("Invalid track id")),
                    }),
                },
                None => Err(TrackSourceError::InvalidSource),
            }
        }
        TrackApiSource::Tidal => {
            let quality = quality.map(|q| q.into()).unwrap_or(TidalAudioQuality::High);
            let track_id = (track_id as u64).into();
            Ok(TrackSource::Tidal {
                url: moosicbox_tidal::track_file_url(db, quality, &track_id, None)
                    .await?
                    .first()
                    .unwrap()
                    .to_string(),
                format: track.and_then(|x| x.format).unwrap_or(AudioFormat::Source),
                track_id: Some(track_id.into()),
            })
        }
        TrackApiSource::Qobuz => {
            let quality = quality.map(|q| q.into()).unwrap_or(QobuzAudioQuality::Low);
            let track_id = (track_id as u64).into();
            Ok(TrackSource::Qobuz {
                url: moosicbox_qobuz::track_file_url(db, &track_id, quality, None, None, None)
                    .await?,
                format: track.and_then(|x| x.format).unwrap_or(AudioFormat::Source),
                track_id: Some(track_id.into()),
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum GetTrackBytesError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    ToStr(#[from] reqwest::header::ToStrError),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Acquire(#[from] tokio::sync::AcquireError),
    #[error(transparent)]
    TrackInfo(#[from] TrackInfoError),
    #[error("Track not found")]
    NotFound,
    #[error("Unsupported format")]
    UnsupportedFormat,
}

#[derive(Debug, Error)]
pub enum TrackByteStreamError {
    #[error("Unknown {0:?}")]
    UnsupportedFormat(Box<dyn std::error::Error>),
}

pub type BytesStreamItem = Result<Bytes, std::io::Error>;
pub type BytesStream = Pin<Box<dyn Stream<Item = BytesStreamItem> + Send>>;

pub struct TrackBytes {
    pub id: usize,
    pub stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
    pub size: Option<u64>,
    pub original_size: Option<u64>,
    pub format: AudioFormat,
    pub filename: Option<String>,
}

pub async fn get_track_bytes(
    db: &dyn Database,
    track_id: u64,
    source: TrackSource,
    format: AudioFormat,
    try_to_get_size: bool,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!("Getting track bytes track_id={track_id} format={format:?} try_to_get_size={try_to_get_size} start={start:?} end={end:?}");

    let size = if try_to_get_size {
        match get_or_init_track_size(track_id as i32, &source, PlaybackQuality { format }, db).await
        {
            Ok(size) => Some(size),
            Err(err) => match err {
                TrackInfoError::UnsupportedFormat(_) | TrackInfoError::UnsupportedSource(_) => None,
                TrackInfoError::NotFound(_) => {
                    log::error!("get_track_bytes error: {err:?}");
                    return Err(GetTrackBytesError::NotFound);
                }
                _ => {
                    log::error!("get_track_bytes error: {err:?}");
                    return Err(GetTrackBytesError::TrackInfo(err));
                }
            },
        }
    } else {
        None
    };

    let track = moosicbox_core::sqlite::db::get_track(db, track_id)
        .await?
        .ok_or(GetTrackBytesError::NotFound)?;

    let format = match format {
        #[cfg(feature = "flac")]
        AudioFormat::Flac => {
            if track.format != Some(AudioFormat::Flac) {
                return Err(GetTrackBytesError::UnsupportedFormat);
            }
            format
        }
        AudioFormat::Source => track.format.ok_or(GetTrackBytesError::UnsupportedFormat)?,
        #[allow(unreachable_patterns)]
        _ => format,
    };

    get_audio_bytes(source, format, size, start, end).await
}

pub async fn get_audio_bytes(
    source: TrackSource,
    format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!("Getting audio bytes format={format:?} size={size:?} start={start:?} end={end:?}");

    get_or_fetch_track(&source, format, size, start, end, {
        let source = source.clone();
        move |start, end, size| {
            let source = source.clone();
            Box::pin(async move {
                log::debug!("get_audio_bytes: cache miss; eagerly fetching audio bytes");
                let writer = ByteWriter::default();
                let writer_id = writer.id;
                #[allow(unused)]
                let stream = writer.stream();
                let same_format = source.format() == format;

                let track_bytes = if same_format {
                    match source {
                        TrackSource::LocalFilePath { path, .. } => {
                            request_audio_bytes_from_file(path, format, size, start, end).await?
                        }
                        TrackSource::Tidal { url, .. } | TrackSource::Qobuz { url, .. } => {
                            request_track_bytes_from_url(&url, start, end, format, size).await?
                        }
                    }
                } else {
                    let source_send = source.clone();

                    RT.spawn_blocking(move || {
                        let source = source_send;

                        let audio_output_handler =
                            match format {
                                #[cfg(feature = "aac")]
                                AudioFormat::Aac => {
                                    use moosicbox_symphonia_player::output::encoder::aac::encoder::AacEncoder;
                                    Some(
                                        moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                            .with_output(Box::new(move |spec, duration| {
                                                Ok(Box::new(
                                                    AacEncoder::with_writer(writer.clone())
                                                        .open(spec, duration),
                                                ))
                                            })),
                                    )
                                }
                                #[cfg(feature = "flac")]
                                AudioFormat::Flac => {
                                    use moosicbox_symphonia_player::output::encoder::flac::encoder::FlacEncoder;
                                    Some(
                                        moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                            .with_output(Box::new(move |spec, duration| {
                                                Ok(Box::new(
                                                    FlacEncoder::with_writer(writer.clone())
                                                        .open(spec, duration),
                                                ))
                                            })),
                                    )
                                }
                                #[cfg(feature = "mp3")]
                                AudioFormat::Mp3 => {
                                    use moosicbox_symphonia_player::output::encoder::mp3::encoder::Mp3Encoder;
                                    let encoder_writer = writer.clone();
                                    Some(
                                        moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                            .with_output(Box::new(move |spec, duration| {
                                                Ok(Box::new(
                                                    Mp3Encoder::with_writer(encoder_writer.clone())
                                                        .open(spec, duration),
                                                ))
                                            })),
                                    )
                                }
                                #[cfg(feature = "opus")]
                                AudioFormat::Opus => {
                                    use moosicbox_symphonia_player::output::encoder::opus::encoder::OpusEncoder;
                                    let encoder_writer = writer.clone();
                                    Some(
                                        moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                            .with_output(Box::new(move |spec, duration| {
                                                Ok(Box::new(
                                                    OpusEncoder::with_writer(encoder_writer.clone())
                                                        .open(spec, duration),
                                                ))
                                            })),
                                    )
                                }
                                AudioFormat::Source => None,
                            };

                        if let Some(mut audio_output_handler) = audio_output_handler {
                            match source {
                                TrackSource::LocalFilePath { ref path, .. } => {
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
                                TrackSource::Tidal { url, .. } | TrackSource::Qobuz { url, .. } => {
                                    let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
                                        url,
                                        size,
                                        true,
                                        #[cfg(feature = "flac")]
                                        {
                                            format == AudioFormat::Flac
                                        },
                                        #[cfg(not(feature = "flac"))]
                                        false,
                                        CancellationToken::new(),
                                    )
                                    .into();
                                    if let Err(err) = play_media_source(
                                        MediaSourceStream::new(Box::new(source), Default::default()),
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
                    })
                    .await?;

                    match source {
                        TrackSource::LocalFilePath { path, .. } => match format {
                            AudioFormat::Source => {
                                request_audio_bytes_from_file(path, format, size, start, end)
                                    .await?
                            }
                            #[allow(unreachable_patterns)]
                            _ => TrackBytes {
                                id: writer_id,
                                stream: StalledReadMonitor::new(stream.boxed()),
                                size,
                                original_size: size,
                                format,
                                filename: filename_from_path_str(&path),
                            },
                        },
                        TrackSource::Tidal { url, .. } | TrackSource::Qobuz { url, .. } => {
                            match format {
                                AudioFormat::Source => {
                                    request_track_bytes_from_url(&url, start, end, format, size)
                                        .await?
                                }
                                #[allow(unreachable_patterns)]
                                _ => TrackBytes {
                                    id: writer_id,
                                    stream: StalledReadMonitor::new(stream.boxed()),
                                    size,
                                    original_size: size,
                                    format,
                                    filename: None,
                                },
                            }
                        }
                    }
                };

                Ok(track_bytes)
            })
        }
    })
    .await
}

async fn request_audio_bytes_from_file(
    path: String,
    format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, std::io::Error> {
    log::debug!("request_audio_bytes_from_file path={path} format={format} size={size:?} start={start:?} end={end:?}");
    let mut file = tokio::fs::File::open(&path).await?;

    if let Some(start) = start {
        file.seek(std::io::SeekFrom::Start(start)).await?;
    }

    let original_size = size;
    let size = if let (Some(start), Some(end)) = (start, end) {
        Some(end - start)
    } else if let Some(start) = start {
        size.map(|size| size - start)
    } else if let Some(end) = end {
        Some(end)
    } else {
        size
    };

    log::debug!("request_audio_bytes_from_file calculated size={size:?}");

    let framed_read = if let Some(size) = size {
        FramedRead::with_capacity(file, BytesCodec::new(), size as usize)
    } else {
        FramedRead::new(file, BytesCodec::new())
    };

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(framed_read.map_ok(BytesMut::freeze).boxed()),
        size,
        original_size,
        format,
        filename: filename_from_path_str(&path),
    })
}

async fn request_track_bytes_from_url(
    url: &str,
    start: Option<u64>,
    end: Option<u64>,
    format: AudioFormat,
    size: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    let client = reqwest::Client::new();

    log::debug!("request_track_bytes_from_url: Getting track source from url: {url}");

    let mut head_request = client.head(url);
    let mut request = client.get(url);

    if start.is_some() || end.is_some() {
        let start = start.map_or("".into(), |start| start.to_string());
        let end = end.map_or("".into(), |end| end.to_string());

        log::debug!("request_track_bytes_from_url: Using byte range start={start} end={end}");
        request = request.header("Range", format!("bytes={start}-{end}"));
        head_request = head_request.header("Range", format!("bytes={start}-{end}"));
    }

    let size = if size.is_none() {
        log::debug!("request_track_bytes_from_url: Sending head request to url={url}");
        let head = head_request.send().await?;

        if let Some(header) = head
            .headers()
            .get(actix_web::http::header::CONTENT_LENGTH.to_string())
        {
            let size = header.to_str()?.parse::<u64>()?;
            log::debug!("Got size from Content-Length header: size={size}");
            Some(size)
        } else {
            log::debug!("No Content-Length header");
            None
        }
    } else {
        log::debug!("Already has size={size:?}");
        size
    };

    log::debug!("request_track_bytes_from_url: Sending request to url={url}");
    let stream = request
        .send()
        .await?
        .bytes_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(stream.boxed()),
        size,
        original_size: size,
        format,
        filename: None,
    })
}

#[derive(Debug, Error)]
pub enum TrackInfoError {
    #[error("Format not supported: {0:?}")]
    UnsupportedFormat(AudioFormat),
    #[error("Source not supported: {0:?}")]
    UnsupportedSource(TrackSource),
    #[error("Track not found: {0}")]
    NotFound(u64),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    GetTrackBytes(#[from] Box<GetTrackBytesError>),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: i32,
    pub blur: bool,
}

impl From<LibraryTrack> for TrackInfo {
    fn from(value: LibraryTrack) -> Self {
        TrackInfo {
            id: value.id,
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            date_released: value.date_released,
            artist: value.artist,
            artist_id: value.artist_id,
            blur: value.blur,
        }
    }
}

pub async fn get_tracks_info(
    track_ids: Vec<u64>,
    db: &dyn Database,
) -> Result<Vec<TrackInfo>, TrackInfoError> {
    log::debug!("Getting tracks info {track_ids:?}");

    let tracks = get_tracks(db, Some(&track_ids)).await?;

    log::trace!("Got tracks {tracks:?}");

    Ok(tracks.into_iter().map(|t| t.into()).collect())
}

pub async fn get_track_info(track_id: u64, db: &dyn Database) -> Result<TrackInfo, TrackInfoError> {
    log::debug!("Getting track info {track_id}");

    let track = get_track(db, track_id).await?;

    log::trace!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackInfoError::NotFound(track_id));
    }

    Ok(track.unwrap().into())
}

const DIV: u16 = u16::MAX / u8::MAX as u16;

pub fn visualize<S>(input: &AudioBuffer<S>) -> Vec<u8>
where
    S: Sample + IntoSample<i16>,
{
    let channels = input.spec().channels.count();

    let mut values = vec![0; input.capacity()];

    for c in 0..channels {
        for (i, x) in input.chan(c).iter().enumerate() {
            let value = clamp_i16(((*x).into_sample() as i32).abs()) as u16;
            values[i] += (value / DIV) as u8;
        }
    }

    for value in values.iter_mut() {
        *value /= channels as u8;
    }

    values
}

pub async fn get_or_init_track_visualization(
    source: &TrackSource,
    max: u16,
) -> Result<Vec<u8>, TrackInfoError> {
    log::debug!(
        "Getting track visualization track_id={:?} max={max}",
        source.track_id()
    );

    let viz = Arc::new(RwLock::new(vec![]));
    let inner_viz = viz.clone();

    let bytes = get_audio_bytes(source.clone(), source.format(), None, None, None)
        .await
        .map_err(Box::new)?;

    tokio::task::spawn_blocking(move || {
        let mut audio_output_handler =
            AudioOutputHandler::new().with_filter(Box::new(move |decoded, _packet, _track| {
                inner_viz
                    .write()
                    .unwrap()
                    .extend_from_slice(&visualize(decoded));
                Ok(())
            }));

        let hint = Hint::new();
        let media_source = TrackBytesMediaSource::new(bytes);
        let mss = MediaSourceStream::new(Box::new(media_source), Default::default());

        play_media_source(
            mss,
            &hint,
            &mut audio_output_handler,
            true,
            true,
            None,
            None,
        )
    })
    .await??;

    let viz = viz.read().unwrap();
    let count = std::cmp::min(max as usize, viz.len());
    let mut ret_viz = Vec::with_capacity(count);

    if viz.len() > max as usize {
        let offset = (viz.len() as f64) / (max as f64);
        log::debug!("Trimming visualization: offset={offset}");
        let mut last_pos = 0_usize;
        let mut pos = offset;

        while (pos as usize) < viz.len() {
            let pos_usize = pos as usize;
            let mut sum = viz[last_pos] as usize;
            let mut count = 1_usize;

            while pos_usize > last_pos {
                last_pos += 1;
                count += 1;
                sum += viz[last_pos] as usize;
            }

            ret_viz.push((sum / count) as u8);
            pos += offset;
        }

        if ret_viz.len() < max as usize {
            ret_viz.push(viz[viz.len() - 1]);
        }
    } else {
        ret_viz.extend_from_slice(&viz[..count]);
    }

    let mut min_value = u8::MAX;
    let mut max_value = 0;

    for x in ret_viz.iter() {
        let x = *x;

        if x < min_value {
            min_value = x;
        }
        if x > max_value {
            max_value = x;
        }
    }

    let dyn_range = max_value - min_value;
    let coefficient = u8::MAX as f64 / dyn_range as f64;

    log::debug!("dyn_range={dyn_range} coefficient={coefficient} min_value={min_value} max_value={max_value}");

    for x in ret_viz.iter_mut() {
        *x -= min_value;
        let diff = *x as f64 * coefficient;
        *x = diff as u8;
    }

    let mut smooth_viz = vec![0; ret_viz.len()];
    let mut last = 0;

    const MAX_DELTA: i16 = 50;

    for (i, x) in smooth_viz.iter_mut().enumerate() {
        let mut current = ret_viz[i] as i16;

        if i > 0 && (current - last).abs() > MAX_DELTA {
            if current > last {
                current = last + MAX_DELTA;
            } else {
                current = last - MAX_DELTA;
            }
        }

        last = current;
        *x = current as u8;
    }

    let ret_viz = smooth_viz;

    Ok(ret_viz)
}

pub async fn get_or_init_track_size(
    track_id: i32,
    source: &TrackSource,
    quality: PlaybackQuality,
    db: &dyn Database,
) -> Result<u64, TrackInfoError> {
    log::debug!("Getting track size track_id={track_id}");

    if let Some(size) = get_track_size(db, track_id as u64, &quality).await? {
        return Ok(size);
    }

    let bytes = match source {
        TrackSource::LocalFilePath { ref path, .. } => match quality.format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => {
                let writer = moosicbox_stream_utils::ByteWriter::default();
                moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            #[cfg(feature = "flac")]
            AudioFormat::Flac => return Err(TrackInfoError::UnsupportedFormat(quality.format)),
            #[cfg(feature = "mp3")]
            AudioFormat::Mp3 => {
                let writer = moosicbox_stream_utils::ByteWriter::default();
                moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            #[cfg(feature = "opus")]
            AudioFormat::Opus => {
                let writer = moosicbox_stream_utils::ByteWriter::default();
                moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            AudioFormat::Source => File::open(path).unwrap().metadata().unwrap().len(),
        },
        TrackSource::Tidal { .. } | TrackSource::Qobuz { .. } => {
            return Err(TrackInfoError::UnsupportedSource(source.clone()))
        }
    };

    set_track_size(
        db,
        SetTrackSize {
            track_id,
            quality,
            bytes: Some(Some(bytes)),
            bit_depth: Some(None),
            audio_bitrate: Some(None),
            overall_bitrate: Some(None),
            sample_rate: Some(None),
            channels: Some(None),
        },
    )
    .await?;

    Ok(bytes)
}
