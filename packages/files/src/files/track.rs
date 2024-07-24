use std::{
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::{Bytes, BytesMut};
use flume::RecvError;
use futures::prelude::*;
use futures_core::Stream;
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{ApiSource, Id, Track},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_music_api::{
    MusicApi, MusicApis, MusicApisError, TrackAudioQuality, TrackError, TrackSource, TracksError,
};
use moosicbox_stream_utils::{
    new_byte_writer_id, remote_bytestream::RemoteByteStream, stalled_monitor::StalledReadMonitor,
    ByteWriter,
};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStreamMediaSource, play_file_path_str_async,
    play_media_source_async, PlaybackError,
};
use serde::{Deserialize, Serialize};
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

use super::track_pool::service::CommanderError;

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

#[derive(Debug, Error)]
pub enum TrackSourceError {
    #[error("Track not found: {0}")]
    NotFound(Id),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    MusicApis(#[from] MusicApisError),
}

pub async fn get_track_id_source(
    apis: MusicApis,
    track_id: &Id,
    source: ApiSource,
    quality: Option<TrackAudioQuality>,
) -> Result<TrackSource, TrackSourceError> {
    let track_api = apis.get(source)?;

    log::debug!(
        "get_track_id_source: track_id={track_id} quality={quality:?} source={:?}",
        track_api.source()
    );

    let track = track_api
        .track(track_id)
        .await?
        .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?;

    let track_source = track.source.into();

    let (api, track) = if track_source != source {
        let api = apis.get(track_source)?;

        (
            api.clone(),
            api.track(
                track
                    .sources
                    .get(track_source)
                    .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?,
            )
            .await?
            .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?,
        )
    } else {
        (track_api, track)
    };

    get_track_source(&**api, &track, quality).await
}

pub async fn get_track_source(
    api: &dyn MusicApi,
    track: &Track,
    quality: Option<TrackAudioQuality>,
) -> Result<TrackSource, TrackSourceError> {
    log::debug!(
        "get_track_source: track_id={:?} quality={quality:?} source={:?}",
        &track.id,
        api.source(),
    );

    log::debug!("Got track {track:?}. Getting source={:?}", api.source());

    api.track_source(
        track.into(),
        quality.unwrap_or(TrackAudioQuality::FlacHighestRes),
    )
    .await?
    .ok_or_else(|| TrackSourceError::NotFound(track.id.to_owned()))
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
    Recv(#[from] RecvError),
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    TrackInfo(#[from] TrackInfoError),
    #[error(transparent)]
    Commander(#[from] CommanderError),
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

impl std::fmt::Debug for TrackBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackBytes")
            .field("id", &self.id)
            .field("stream", &"{{stream}}")
            .field("size", &self.size)
            .field("original_size", &self.original_size)
            .field("format", &self.format)
            .field("filename", &self.filename)
            .finish()
    }
}

pub async fn get_track_bytes(
    api: &dyn MusicApi,
    track_id: &Id,
    source: TrackSource,
    format: AudioFormat,
    try_to_get_size: bool,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!("get_track_bytes: Getting track bytes track_id={track_id} format={format:?} try_to_get_size={try_to_get_size} start={start:?} end={end:?}");

    let size = if try_to_get_size {
        match get_or_init_track_size(api, track_id, &source, PlaybackQuality { format }).await {
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

    log::debug!("get_track_bytes: Got track size: size={size:?} track_id={track_id}");

    let track = api
        .track(track_id)
        .await?
        .ok_or(GetTrackBytesError::NotFound)?;

    log::debug!("get_track_bytes: Got track from api: track={track:?}");

    let format = match format {
        #[cfg(feature = "flac")]
        AudioFormat::Flac => {
            if track.format != Some(AudioFormat::Flac) {
                return Err(GetTrackBytesError::UnsupportedFormat);
            }
            format
        }
        AudioFormat::Source => format,
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
                        TrackSource::RemoteUrl { url, .. } => {
                            request_track_bytes_from_url(&url, start, end, format, size).await?
                        }
                    }
                } else {
                    let get_handler = move || {
                        #[allow(unreachable_code)]
                        Ok(match format {
                            #[cfg(feature = "aac")]
                            AudioFormat::Aac => {
                                use moosicbox_symphonia_player::output::encoders::aac::AacEncoder;
                                moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                    .with_output(Box::new(move |spec, duration| {
                                        Ok(Box::new(
                                            AacEncoder::with_writer(writer.clone())
                                                .open(spec, duration),
                                        ))
                                    }))
                            }
                            #[cfg(feature = "flac")]
                            AudioFormat::Flac => {
                                use moosicbox_symphonia_player::output::encoders::flac::FlacEncoder;
                                moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                    .with_output(Box::new(move |spec, duration| {
                                        Ok(Box::new(
                                            FlacEncoder::with_writer(writer.clone())
                                                .open(spec, duration),
                                        ))
                                    }))
                            }
                            #[cfg(feature = "mp3")]
                            AudioFormat::Mp3 => {
                                use moosicbox_symphonia_player::output::encoders::mp3::Mp3Encoder;
                                let encoder_writer = writer.clone();
                                moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                    .with_output(Box::new(move |spec, duration| {
                                        Ok(Box::new(
                                            Mp3Encoder::with_writer(encoder_writer.clone())
                                                .open(spec, duration),
                                        ))
                                    }))
                            }
                            #[cfg(feature = "opus")]
                            AudioFormat::Opus => {
                                use moosicbox_symphonia_player::output::encoders::opus::OpusEncoder;
                                let encoder_writer = writer.clone();
                                moosicbox_symphonia_player::output::AudioOutputHandler::new()
                                    .with_output(Box::new(move |spec, duration| {
                                        Ok(Box::new(
                                            OpusEncoder::with_writer(encoder_writer.clone())
                                                .open(spec, duration),
                                        ))
                                    }))
                            }
                            AudioFormat::Source => {
                                return Err(
                                    moosicbox_symphonia_player::PlaybackError::InvalidSource,
                                )
                            }
                        })
                    };

                    match source {
                        TrackSource::LocalFilePath { ref path, .. } => {
                            if let Err(err) =
                                play_file_path_str_async(path, get_handler, true, true, None, None)
                                    .await
                            {
                                log::error!("Failed to encode to aac: {err:?}");
                            }
                        }
                        TrackSource::RemoteUrl { ref url, .. } => {
                            let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
                                url.to_string(),
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
                            if let Err(err) = play_media_source_async(
                                MediaSourceStream::new(Box::new(source), Default::default()),
                                &Hint::new(),
                                get_handler,
                                true,
                                true,
                                None,
                                None,
                            )
                            .await
                            {
                                log::error!("Failed to encode to aac: {err:?}");
                            }
                        }
                    }

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
                        TrackSource::RemoteUrl { url, .. } => match format {
                            AudioFormat::Source => {
                                request_track_bytes_from_url(&url, start, end, format, size).await?
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
                        },
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

    let original_size = if let Some(size) = size {
        size
    } else {
        file.metadata().await?.len()
    };

    let size = if let (Some(start), Some(end)) = (start, end) {
        end - start
    } else if let Some(start) = start {
        original_size - start
    } else if let Some(end) = end {
        end
    } else if let Some(size) = size {
        size
    } else {
        original_size
    };

    log::debug!(
        "request_audio_bytes_from_file calculated size={size} original_size={original_size}"
    );

    let framed_read = FramedRead::with_capacity(file, BytesCodec::new(), size as usize);

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(framed_read.map_ok(BytesMut::freeze).boxed()),
        size: Some(size),
        original_size: Some(original_size),
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
    NotFound(Id),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    GetTrackBytes(#[from] Box<GetTrackBytesError>),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Tracks(#[from] TracksError),
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: Id,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: Id,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: Id,
    pub blur: bool,
}

impl From<Track> for TrackInfo {
    fn from(value: Track) -> Self {
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
    api: &dyn MusicApi,
    track_ids: &[Id],
) -> Result<Vec<TrackInfo>, TrackInfoError> {
    log::debug!("Getting tracks info {track_ids:?}");

    let tracks = api
        .tracks(Some(track_ids), None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    log::trace!("Got tracks {tracks:?}");

    Ok(tracks.into_iter().map(|t| t.into()).collect())
}

pub async fn get_track_info(
    api: &dyn MusicApi,
    track_id: &Id,
) -> Result<TrackInfo, TrackInfoError> {
    log::debug!("Getting track info {track_id}");

    let track = api.track(track_id).await?;

    log::trace!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackInfoError::NotFound(track_id.to_owned()));
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

    let get_handler = move || {
        Ok(
            moosicbox_symphonia_player::output::AudioOutputHandler::new().with_filter(Box::new(
                move |decoded, _packet, _track| {
                    inner_viz
                        .write()
                        .unwrap()
                        .extend_from_slice(&visualize(decoded));
                    Ok(())
                },
            )),
        )
    };

    let hint = Hint::new();
    let media_source = TrackBytesMediaSource::new(bytes);
    let mss = MediaSourceStream::new(Box::new(media_source), Default::default());

    play_media_source_async(mss, &hint, get_handler, true, true, None, None).await?;

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
    api: &dyn MusicApi,
    track_id: &Id,
    source: &TrackSource,
    quality: PlaybackQuality,
) -> Result<u64, TrackInfoError> {
    log::debug!("Getting track size track_id={track_id}");

    api.track_size(track_id.into(), source, quality)
        .await?
        .ok_or_else(|| TrackInfoError::NotFound(track_id.to_owned()))
}
