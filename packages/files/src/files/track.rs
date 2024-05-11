use std::{
    env,
    fs::File,
    pin::Pin,
    str::FromStr,
    sync::{Arc, RwLock},
};

use bytes::{Bytes, BytesMut};
use futures::prelude::*;
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
use moosicbox_stream_utils::{stalled_monitor::StalledReadMonitor, ByteWriter};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStream, output::AudioOutputHandler,
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
use tokio_util::{
    codec::{BytesCodec, FramedRead},
    sync::CancellationToken,
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
    LocalFilePath { path: String, format: AudioFormat },
    Tidal { url: String, format: AudioFormat },
    Qobuz { url: String, format: AudioFormat },
}

impl TrackSource {
    pub fn format(&self) -> AudioFormat {
        match self {
            TrackSource::LocalFilePath { format, .. } => *format,
            TrackSource::Tidal { format, .. } => *format,
            TrackSource::Qobuz { format, .. } => *format,
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
    source: Option<TrackApiSource>,
) -> Result<TrackSource, TrackSourceError> {
    log::debug!("Getting track audio file {track_id} quality={quality:?} source={source:?}");

    let track = get_track(db, track_id as u64)
        .await?
        .ok_or(TrackSourceError::NotFound(track_id))?;

    get_track_source(&track, db, quality, source).await
}

pub async fn get_track_source(
    track: &LibraryTrack,
    db: &dyn Database,
    quality: Option<TrackAudioQuality>,
    source: Option<TrackApiSource>,
) -> Result<TrackSource, TrackSourceError> {
    log::debug!(
        "Getting track audio file {} quality={quality:?} source={source:?}",
        track.id
    );

    let source = source.unwrap_or(track.source);

    log::debug!("Got track {track:?}. Getting source={source:?}");

    match source {
        TrackApiSource::Local => match &track.file {
            Some(file) => match env::consts::OS {
                "windows" => Ok(TrackSource::LocalFilePath {
                    path: Regex::new(r"/mnt/(\w+)")
                        .unwrap()
                        .replace(file, |caps: &Captures| {
                            format!("{}:", caps[1].to_uppercase())
                        })
                        .replace('/', "\\"),
                    format: track.format.unwrap_or(AudioFormat::Source),
                }),
                _ => Ok(TrackSource::LocalFilePath {
                    path: file.to_string(),
                    format: track.format.unwrap_or(AudioFormat::Source),
                }),
            },
            None => Err(TrackSourceError::InvalidSource),
        },
        TrackApiSource::Tidal => {
            let quality = quality.map(|q| q.into()).unwrap_or(TidalAudioQuality::High);
            let track_id = track
                .tidal_id
                .ok_or(TrackSourceError::InvalidSource)?
                .into();
            Ok(TrackSource::Tidal {
                url: moosicbox_tidal::track_file_url(db, quality, &track_id, None)
                    .await?
                    .first()
                    .unwrap()
                    .to_string(),
                format: track.format.unwrap_or(AudioFormat::Source),
            })
        }
        TrackApiSource::Qobuz => {
            let quality = quality.map(|q| q.into()).unwrap_or(QobuzAudioQuality::Low);
            let track_id = track
                .qobuz_id
                .ok_or(TrackSourceError::InvalidSource)?
                .into();
            Ok(TrackSource::Qobuz {
                url: moosicbox_qobuz::track_file_url(db, &track_id, quality, None, None, None)
                    .await?,
                format: track.format.unwrap_or(AudioFormat::Source),
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum GetTrackBytesError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
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

type BytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

pub struct TrackBytes {
    pub stream: StalledReadMonitor<Result<Bytes, std::io::Error>, BytesStream>,
    pub size: Option<u64>,
    pub format: AudioFormat,
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
    log::debug!("Getting audio bytes format={format:?} start={start:?} end={end:?}");

    let writer = ByteWriter::default();
    #[allow(unused)]
    let stream = writer.stream();
    let same_format = source.format() == format;

    let track_bytes = if same_format {
        match source {
            TrackSource::LocalFilePath { path, .. } => {
                request_audio_bytes_from_file(path, format, size).await?
            }
            TrackSource::Tidal { url, .. } | TrackSource::Qobuz { url, .. } => {
                request_track_bytes_from_url(&url, start, end, format, size).await?
            }
        }
    } else {
        let source_send = source.clone();

        RT.spawn(async move {
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
                        let source = Box::new(RemoteByteStream::new(
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

        match source {
            TrackSource::LocalFilePath { path, .. } => match format {
                AudioFormat::Source => request_audio_bytes_from_file(path, format, size).await?,
                #[allow(unreachable_patterns)]
                _ => TrackBytes {
                    stream: StalledReadMonitor::new(stream.boxed()),
                    size,
                    format,
                },
            },
            TrackSource::Tidal { url, .. } | TrackSource::Qobuz { url, .. } => match format {
                AudioFormat::Source => {
                    request_track_bytes_from_url(&url, start, end, format, size).await?
                }
                #[allow(unreachable_patterns)]
                _ => TrackBytes {
                    stream: StalledReadMonitor::new(stream.boxed()),
                    size,
                    format,
                },
            },
        }
    };

    Ok(track_bytes)
}

async fn request_audio_bytes_from_file(
    path: String,
    format: AudioFormat,
    size: Option<u64>,
) -> Result<TrackBytes, std::io::Error> {
    let file = tokio::fs::File::open(path).await?;

    Ok(TrackBytes {
        stream: StalledReadMonitor::new(
            FramedRead::new(file, BytesCodec::new())
                .map_ok(BytesMut::freeze)
                .boxed(),
        ),
        size,
        format,
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

    log::debug!("Getting track source from url: {url}");

    let mut request = client.get(url);

    if start.is_some() || end.is_some() {
        let start = start.map(|start| start.to_string()).unwrap_or("".into());
        let end = end.map(|end| end.to_string()).unwrap_or("".into());

        request = request.header("Range", format!("bytes={start}-{end}"))
    }

    let stream = request
        .send()
        .await?
        .bytes_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));

    Ok(TrackBytes {
        stream: StalledReadMonitor::new(stream.boxed()),
        size,
        format,
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
    Playback(#[from] PlaybackError),
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

pub fn visualize<S>(input: &AudioBuffer<S>) -> u8
where
    S: Sample + IntoSample<i16>,
{
    let channels = input.spec().channels.count();

    let mut step = 1_u16;
    let mut count = 0_u16;
    let mut max = 0_u16;

    for c in 0..channels {
        for x in input.chan(c) {
            let value = clamp_i16(((*x).into_sample() as i32).abs()) as u16;
            max = std::cmp::max(value, max);
            count += 1;
            if count >= step {
                step += step;
                break;
            }
        }
    }

    if count == 0 {
        return 0;
    }

    (max / DIV) as u8
}

pub fn get_or_init_track_visualization(
    track_id: i32,
    source: &TrackSource,
    max: u16,
) -> Result<Vec<u8>, TrackInfoError> {
    log::debug!("Getting track visualization {track_id} max={max}");

    match source {
        TrackSource::LocalFilePath { ref path, .. } => {
            let viz = Arc::new(RwLock::new(vec![]));
            let inner_viz = viz.clone();
            let mut audio_output_handler =
                AudioOutputHandler::new().with_filter(Box::new(move |decoded, _packet, _track| {
                    inner_viz.write().unwrap().push(visualize(decoded));
                    Ok(())
                }));

            play_file_path_str(path, &mut audio_output_handler, true, true, None, None)?;

            let viz = viz.read().unwrap();
            let count = std::cmp::min(max as usize, viz.len());
            let mut ret_viz = Vec::with_capacity(count);

            if viz.len() as u16 > max {
                let offset = (viz.len() as f64) / (max as f64);
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
        TrackSource::Tidal { .. } | TrackSource::Qobuz { .. } => unimplemented!(),
    }
}

pub async fn get_or_init_track_size(
    track_id: i32,
    source: &TrackSource,
    quality: PlaybackQuality,
    db: &dyn Database,
) -> Result<u64, TrackInfoError> {
    log::debug!("Getting track size {track_id}");

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
