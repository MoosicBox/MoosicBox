//! Track audio streaming, metadata, and visualization.
//!
//! Provides core functionality for streaming track audio with format conversion, retrieving track
//! metadata, generating audio visualizations, and managing track byte sources. Supports both local
//! files and remote URLs with optional transcoding between audio formats.

#![allow(clippy::module_name_repetitions)]

use std::{
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use flume::RecvError;
use futures::prelude::*;
use futures_core::Stream;
use moosicbox_audio_decoder::{
    DecodeError, decode_file_path_str_async, decode_media_source_async,
    media_sources::remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_audio_output::{AudioOutputError, AudioWrite, Channels, SignalSpec};
use moosicbox_music_api::{
    MusicApi, MusicApis, SourceToMusicApi as _,
    models::{TrackAudioQuality, TrackSource},
};
use moosicbox_music_models::{ApiSource, AudioFormat, PlaybackQuality, Track, id::Id};
use moosicbox_stream_utils::{
    ByteWriter, new_byte_writer_id, remote_bytestream::RemoteByteStream,
    stalled_monitor::StalledReadMonitor,
};

use serde::{Deserialize, Serialize};
use switchy_async::util::CancellationToken;
use symphonia::core::{
    audio::{AudioBuffer, Signal},
    conv::IntoSample,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    probe::Hint,
    sample::Sample,
    util::clamp::clamp_i16,
};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::files::{
    filename_from_path_str, track_bytes_media_source::TrackBytesMediaSource,
    track_pool::get_or_fetch_track,
};

use super::track_pool::service::CommanderError;

/// Converts a track source to its HTTP content-type header value.
///
/// Returns the appropriate MIME type based on the track's audio format.
#[must_use]
pub fn track_source_to_content_type(source: &TrackSource) -> Option<String> {
    audio_format_to_content_type(&source.format())
}

/// Converts an audio format to its HTTP content-type header value.
///
/// Returns the appropriate MIME type for the given audio format. Returns `None` for
/// `AudioFormat::Source` as the format is determined by the source file.
#[must_use]
#[allow(clippy::missing_const_for_fn)]
pub fn audio_format_to_content_type(format: &AudioFormat) -> Option<String> {
    match format {
        #[cfg(feature = "format-aac")]
        AudioFormat::Aac => Some("audio/m4a".into()),
        #[cfg(feature = "format-flac")]
        AudioFormat::Flac => Some("audio/flac".into()),
        #[cfg(feature = "format-mp3")]
        AudioFormat::Mp3 => Some("audio/mp3".into()),
        #[cfg(feature = "format-opus")]
        AudioFormat::Opus => Some("audio/opus".into()),
        AudioFormat::Source => None,
    }
}

/// Errors that can occur when retrieving track source information.
#[derive(Debug, Error)]
pub enum TrackSourceError {
    /// Track not found with the specified ID
    #[error("Track not found: {0}")]
    NotFound(Id),
    /// Invalid or unsupported API source
    #[error("Invalid source")]
    InvalidSource,
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// Retrieves the track source information for a given track ID.
///
/// Resolves the track through the music API and returns the appropriate source (local file or
/// remote URL) for streaming the track at the requested quality.
///
/// # Errors
///
/// * `TrackSourceError::NotFound` - If the track was not found
/// * `TrackSourceError::MusicApi` - If failed to get the track info
/// * `TrackSourceError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_track_id_source(
    apis: MusicApis,
    track_id: &Id,
    source: ApiSource,
    quality: Option<TrackAudioQuality>,
) -> Result<TrackSource, TrackSourceError> {
    let track_api = apis
        .get(&source)
        .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?;

    log::debug!(
        "get_track_id_source: track_id={track_id} quality={quality:?} source={:?}",
        track_api.source()
    );

    let track = track_api
        .track(track_id)
        .await?
        .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?;

    log::debug!("get_track_id_source: track={track:?}");

    let track_source: ApiSource = track.track_source.clone().into();

    let (api, track) = if track_source == source {
        (track_api, track)
    } else {
        let api = apis
            .get(&track_source)
            .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?;

        (
            api.clone(),
            api.track(
                track
                    .sources
                    .get(&track_source)
                    .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?,
            )
            .await?
            .ok_or_else(|| TrackSourceError::NotFound(track_id.to_owned()))?,
        )
    };

    get_track_source(&**api, &track, quality).await
}

/// Retrieves the track source from a track object.
///
/// Gets the appropriate source (local file or remote URL) for streaming the track at the
/// requested quality. This is a lower-level function used by `get_track_id_source`.
///
/// # Errors
///
/// * `TrackSourceError::NotFound` - If the track source was not found
/// * `TrackSourceError::MusicApi` - If failed to get the track source
/// * `TrackSourceError::InvalidSource` - If the `ApiSource` is invalid
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
    .ok_or_else(|| TrackSourceError::NotFound(track.id.clone()))
}

/// Errors that can occur when retrieving track audio bytes.
#[derive(Debug, Error)]
pub enum GetTrackBytesError {
    /// Failed to parse integer value
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    /// IO error reading or writing track data
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// HTTP request error
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// Tokio task join error
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    /// Semaphore acquire error
    #[error(transparent)]
    Acquire(#[from] tokio::sync::AcquireError),
    /// Channel receive error
    #[error(transparent)]
    Recv(#[from] RecvError),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Track info retrieval error
    #[error(transparent)]
    TrackInfo(#[from] TrackInfoError),
    /// Commander service error
    #[error(transparent)]
    Commander(#[from] CommanderError),
    /// Track not found
    #[error("Track not found")]
    NotFound,
    /// Unsupported audio format
    #[error("Unsupported format")]
    UnsupportedFormat,
}

/// Errors that can occur when processing track byte streams.
#[derive(Debug, Error)]
pub enum TrackByteStreamError {
    /// Unsupported audio format encountered during stream processing
    #[error("Unknown {0:?}")]
    UnsupportedFormat(Box<dyn std::error::Error>),
}

/// A single item from a byte stream, either bytes or an IO error.
pub type BytesStreamItem = Result<Bytes, std::io::Error>;

/// A pinned, sendable stream of byte chunks.
pub type BytesStream = Pin<Box<dyn Stream<Item = BytesStreamItem> + Send>>;

/// Represents a stream of track audio bytes with metadata.
pub struct TrackBytes {
    /// Unique identifier for this byte stream
    pub id: usize,
    /// Monitored stream of audio bytes
    pub stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
    /// Size of the audio data in bytes (may be unknown for some sources)
    pub size: Option<u64>,
    /// Original size before any transformations (e.g., before encoding)
    pub original_size: Option<u64>,
    /// Audio format of the stream
    pub format: AudioFormat,
    /// Filename of the track file (if available)
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

/// Retrieves the audio bytes for a track as a stream.
///
/// Fetches track audio data from the specified source (local file or remote URL) and returns
/// it as a byte stream. Optionally converts the format if different from the source format.
/// Supports partial content via byte range parameters.
///
/// # Errors
///
/// * `GetTrackBytesError::NotFound` - If the track was not found
/// * `GetTrackBytesError::TrackInfo` - If failed to get the track info
/// * `GetTrackBytesError::IO` - If an IO error occurs
/// * `GetTrackBytesError::MusicApi` - If a music API error occurs
/// * `GetTrackBytesError::UnsupportedFormat` - If the `AudioFormat` is invalid
pub async fn get_track_bytes(
    api: &dyn MusicApi,
    track_id: &Id,
    source: TrackSource,
    format: AudioFormat,
    try_to_get_size: bool,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!(
        "get_track_bytes: Getting track bytes track_id={track_id} format={format:?} try_to_get_size={try_to_get_size} start={start:?} end={end:?}"
    );

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
        #[cfg(feature = "format-flac")]
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

/// Errors that can occur when generating silent audio bytes.
#[derive(Debug, Error)]
pub enum GetSilenceBytesError {
    /// Invalid or unsupported audio source
    #[error("Invalid source")]
    InvalidSource,
    /// Audio output encoding error
    #[error(transparent)]
    AudioOutput(#[from] AudioOutputError),
}

/// Generates silent audio bytes for a specified duration.
///
/// Creates a stream of silent audio in the requested format and duration. Useful for filling
/// gaps in playback or testing audio pipelines.
///
/// # Errors
///
/// * `GetSilenceBytesError::AudioOutput` - If failed to encode the audio bytes
/// * `GetSilenceBytesError::InvalidSource` - If the `AudioFormat` is `Source` (invalid for generated audio)
///
/// # Panics
///
/// * If an encoder feature is not enabled for the `AudioFormat`
pub fn get_silence_bytes(
    format: AudioFormat,
    duration: u64,
) -> Result<TrackBytes, GetSilenceBytesError> {
    log::debug!("get_silence_bytes: format={format:?} duration={duration:?}");
    let writer = ByteWriter::default();
    let writer_id = writer.id;
    #[allow(unused)]
    let stream = writer.stream();

    #[allow(unused)]
    let spec = SignalSpec {
        rate: 44_100,
        channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
    };
    #[allow(unused)]
    let duration: u64 = u64::from(spec.rate) * duration;

    #[allow(unreachable_code)]
    switchy_async::runtime::Handle::current().spawn_blocking_with_name(
        "get_silence_bytes: encode",
        move || {
            #[allow(unused)]
            let mut encoder: Box<dyn AudioWrite> = match format {
                #[cfg(feature = "format-aac")]
                AudioFormat::Aac => {
                    #[cfg(feature = "encoder-aac")]
                    {
                        Box::new(
                            moosicbox_audio_output::encoder::aac::AacEncoder::with_writer(writer)
                                .open(spec, duration),
                        )
                    }
                    #[cfg(not(feature = "encoder-aac"))]
                    panic!("No encoder-aac feature");
                }
                #[cfg(feature = "format-flac")]
                AudioFormat::Flac => {
                    #[cfg(feature = "encoder-flac")]
                    {
                        Box::new(
                            moosicbox_audio_output::encoder::flac::FlacEncoder::with_writer(writer)
                                .open(spec, duration),
                        )
                    }
                    #[cfg(not(feature = "encoder-flac"))]
                    panic!("No encoder-aac feature");
                }
                #[cfg(feature = "format-mp3")]
                AudioFormat::Mp3 => {
                    #[cfg(feature = "encoder-mp3")]
                    {
                        Box::new(
                            moosicbox_audio_output::encoder::mp3::Mp3Encoder::with_writer(writer)
                                .open(spec, duration),
                        )
                    }
                    #[cfg(not(feature = "encoder-mp3"))]
                    panic!("No encoder-mp3 feature");
                }
                #[cfg(feature = "format-opus")]
                AudioFormat::Opus => {
                    #[cfg(feature = "encoder-opus")]
                    {
                        Box::new(
                            moosicbox_audio_output::encoder::opus::OpusEncoder::with_writer(writer)
                                .open(spec, duration),
                        )
                    }
                    #[cfg(not(feature = "encoder-opus"))]
                    panic!("No encoder-opus feature");
                }
                AudioFormat::Source => return Err::<(), _>(GetSilenceBytesError::InvalidSource),
            };

            #[cfg(any(
                feature = "format-aac",
                feature = "format-flac",
                feature = "format-mp3",
                feature = "format-opus"
            ))]
            {
                let mut buffer = AudioBuffer::<f32>::new(duration, spec);
                buffer.render_silence(None);
                encoder.write(buffer)?;
                encoder.flush()?;

                Ok(())
            }
        },
    );

    Ok(TrackBytes {
        id: writer_id,
        stream: StalledReadMonitor::new(stream.boxed()),
        size: None,
        original_size: None,
        format,
        filename: None,
    })
}

/// Retrieves audio bytes from a track source with optional format conversion.
///
/// Low-level function that fetches audio data from local files or remote URLs and optionally
/// transcodes to a different format. Uses the track pool for caching when appropriate.
/// Supports byte range requests for partial content streaming.
///
/// # Errors
///
/// * `GetTrackBytesError::NotFound` - If the track was not found
/// * `GetTrackBytesError::IO` - If an IO error occurs
/// * `GetTrackBytesError::Http` - If an HTTP request fails
/// * `GetTrackBytesError::Commander` - If track pool service error occurs
/// * `GetTrackBytesError::UnsupportedFormat` - If the `AudioFormat` is invalid
///
/// # Panics
///
/// * If an encoder feature is not enabled for the `AudioFormat`
#[allow(clippy::too_many_lines)]
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
                let same_format = format == AudioFormat::Source || source.format() == format;

                let track_bytes = if same_format {
                    match source {
                        TrackSource::LocalFilePath { path, .. } => {
                            request_audio_bytes_from_file(path, format, size, start, end).await?
                        }
                        TrackSource::RemoteUrl { url, headers, .. } => {
                            request_track_bytes_from_url(&url, headers.as_deref(), start, end, format, size).await?
                        }
                    }
                } else {
                    let get_handler = move || {
                        #[allow(unreachable_code)]
                        Ok(match format {
                            #[cfg(feature = "format-aac")]
                            AudioFormat::Aac => {
                                #[cfg(feature = "encoder-aac")]
                                {
                                    moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(
                                        Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                moosicbox_audio_output::encoder::aac::AacEncoder::with_writer(writer.clone())
                                                    .open(spec, duration),
                                            ))
                                        }),
                                    )
                                }
                                #[cfg(not(feature = "encoder-aac"))]
                                panic!("No encoder-aac feature");
                            }
                            #[cfg(feature = "format-flac")]
                            AudioFormat::Flac => {
                                #[cfg(feature = "encoder-flac")]
                                {
                                    moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(
                                        Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                moosicbox_audio_output::encoder::flac::FlacEncoder::with_writer(writer.clone())
                                                    .open(spec, duration),
                                            ))
                                        }),
                                    )
                                }
                                #[cfg(not(feature = "encoder-flac"))]
                                panic!("No encoder-flac feature");
                            }
                            #[cfg(feature = "format-mp3")]
                            AudioFormat::Mp3 => {
                                #[cfg(feature = "encoder-mp3")]
                                {
                                    moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(
                                        Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                moosicbox_audio_output::encoder::mp3::Mp3Encoder::with_writer(writer.clone())
                                                    .open(spec, duration),
                                            ))
                                        }),
                                    )
                                }
                                #[cfg(not(feature = "encoder-mp3"))]
                                panic!("No encoder-mp3 feature");
                            }
                            #[cfg(feature = "format-opus")]
                            AudioFormat::Opus => {
                                #[cfg(feature = "encoder-opus")]
                                {
                                    moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(
                                        Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                moosicbox_audio_output::encoder::opus::OpusEncoder::with_writer(writer.clone())
                                                    .open(spec, duration),
                                            ))
                                        }),
                                    )
                                }
                                #[cfg(not(feature = "encoder-opus"))]
                                panic!("No encoder-opus feature");
                            }
                            AudioFormat::Source => {
                                return Err(moosicbox_audio_decoder::DecodeError::InvalidSource)
                            }
                        })
                    };

                    match &source {
                        TrackSource::LocalFilePath { path, .. } => {
                            if let Err(err) = decode_file_path_str_async(
                                path,
                                get_handler,
                                true,
                                true,
                                None,
                                None,
                            )
                            .await
                            {
                                log::error!(
                                    "Failed to encode to {format} (source={}): {err:?}",
                                    source.format()
                                );
                            }
                        }
                        TrackSource::RemoteUrl { url, .. } => {
                            let source_format = source.format();
                            let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
                                url.clone(),
                                size,
                                true,
                                size.is_some(), // HTTP range requests work for any format when size is known
                                CancellationToken::new(),
                            )
                            .into();
                            if let Err(err) = decode_media_source_async(
                                MediaSourceStream::new(
                                    Box::new(source),
                                    MediaSourceStreamOptions::default(),
                                ),
                                &Hint::new(),
                                get_handler,
                                true,
                                true,
                                None,
                                None,
                            )
                            .await
                            {
                                log::error!(
                                    "Failed to encode to {format} (source={source_format}): {err:?}",
                                );
                            }
                        }
                    }

                    #[allow(clippy::match_wildcard_for_single_variants)]
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
                        TrackSource::RemoteUrl { url, headers, .. } => match format {
                            AudioFormat::Source => {
                                request_track_bytes_from_url(&url, headers.as_deref(), start, end, format, size).await?
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

#[allow(clippy::too_many_lines)]
async fn request_audio_bytes_from_file(
    path: String,
    format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<TrackBytes, std::io::Error> {
    log::debug!(
        "request_audio_bytes_from_file path={path} format={format} size={size:?} start={start:?} end={end:?}"
    );
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

    // Use manual chunk-based reading instead of ReaderStream to eliminate potential truncation issues
    let (sender, receiver) = flume::unbounded();
    let file_path = path.clone();

    switchy_async::runtime::Handle::current().spawn_with_name(
        "files: Manual file reader",
        async move {
            let mut file = match tokio::fs::File::open(&file_path).await {
                Ok(file) => file,
                Err(e) => {
                    log::error!("Failed to open file for manual reading: {e}");
                    return;
                }
            };

            if let Some(start) = start
                && let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await
            {
                log::error!("Failed to seek to start position: {e}");
                return;
            }

            let mut bytes_read = 0u64;
            let mut buffer = vec![0u8; 8192]; // 8KB chunks

            log::debug!("Manual file reader starting for {file_path}, target size: {size}");

            loop {
                match file.read(&mut buffer).await {
                    Ok(0) => {
                        log::debug!(
                            "Manual file reader: EOF reached after reading {bytes_read} bytes"
                        );
                        break;
                    }
                    Ok(n) => {
                        bytes_read += n as u64;

                        // Check if we should stop due to size limit
                        if bytes_read > size {
                            let excess = bytes_read - size;
                            let send_bytes = n - usize::try_from(excess).unwrap();
                            if send_bytes > 0
                                && sender
                                    .send_async(Ok(Bytes::copy_from_slice(&buffer[..send_bytes])))
                                    .await
                                    .is_err()
                            {
                                log::debug!("Manual file reader: receiver dropped (final chunk)");
                            }
                            log::debug!("Manual file reader: reached size limit {size} bytes");
                            break;
                        }

                        log::trace!("Manual file reader: read {n} bytes (total: {bytes_read})");

                        if sender
                            .send_async(Ok(Bytes::copy_from_slice(&buffer[..n])))
                            .await
                            .is_err()
                        {
                            log::debug!("Manual file reader: receiver dropped");
                            break;
                        }
                    }
                    Err(e) => {
                        log::error!("Manual file reader: read error after {bytes_read} bytes: {e}");
                        let _ = sender.send_async(Err(e)).await;
                        break;
                    }
                }
            }

            log::debug!("Manual file reader finished: read {bytes_read} bytes total");
        },
    );

    let stream = futures::stream::unfold(receiver, |receiver| async move {
        receiver.recv_async().await.ok().map(|x| (x, receiver))
    })
    .boxed();

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(stream),
        size: Some(size),
        original_size: Some(original_size),
        format,
        filename: filename_from_path_str(&path),
    })
}

async fn request_track_bytes_from_url(
    url: &str,
    headers: Option<&[(String, String)]>,
    start: Option<u64>,
    end: Option<u64>,
    format: AudioFormat,
    size: Option<u64>,
) -> Result<TrackBytes, GetTrackBytesError> {
    let client = switchy_http::Client::new();

    log::debug!(
        "request_track_bytes_from_url: Getting track source from url={url} headers={headers:?} start={start:?} end={end:?} format={format:?} size={size:?}"
    );

    let mut head_request = client.head(url);
    let mut request = client.get(url);

    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key, value);
            head_request = head_request.header(key, value);
        }
    }

    if start.is_some() || end.is_some() {
        let start = start.map_or_else(String::new, |start| start.to_string());
        let end = end.map_or_else(String::new, |end| end.to_string());

        log::debug!("request_track_bytes_from_url: Using byte range start={start} end={end}");
        request = request.header(
            switchy_http::Header::Range.as_ref(),
            &format!("bytes={start}-{end}"),
        );
        head_request = head_request.header(
            switchy_http::Header::Range.as_ref(),
            &format!("bytes={start}-{end}"),
        );
    }

    let size = if size.is_none() {
        log::debug!("request_track_bytes_from_url: Sending head request to url={url}");
        let mut head = head_request.send().await?;

        if let Some(header) = head
            .headers()
            .get(switchy_http::Header::ContentLength.as_ref())
        {
            let size = header.parse::<u64>()?;
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
        .map_err(std::io::Error::other);

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(stream.boxed()),
        size,
        original_size: size,
        format,
        filename: None,
    })
}

/// Errors that can occur when retrieving track metadata information.
#[derive(Debug, Error)]
pub enum TrackInfoError {
    /// Audio format is not supported for this operation
    #[error("Format not supported: {0:?}")]
    UnsupportedFormat(AudioFormat),
    /// Track source type is not supported for this operation
    #[error("Source not supported: {0:?}")]
    UnsupportedSource(TrackSource),
    /// Track not found with the specified ID
    #[error("Track not found: {0}")]
    NotFound(Id),
    /// Tokio task join error
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    /// Audio decoding error
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// Error retrieving track bytes
    #[error(transparent)]
    GetTrackBytes(#[from] Box<GetTrackBytesError>),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// Track metadata information including ID, title, artist, album, and duration.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    /// Unique track identifier
    pub id: Id,
    /// Track number within the album
    pub number: u32,
    /// Track title
    pub title: String,
    /// Track duration in seconds
    pub duration: f64,
    /// Album name
    pub album: String,
    /// Album identifier
    pub album_id: Id,
    /// Release date (ISO 8601 format if available)
    pub date_released: Option<String>,
    /// Artist name
    pub artist: String,
    /// Artist identifier
    pub artist_id: Id,
    /// Whether the track should be blurred (e.g., explicit content)
    pub blur: bool,
}

impl From<Track> for TrackInfo {
    fn from(value: Track) -> Self {
        Self {
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

/// Retrieves metadata information for multiple tracks.
///
/// Fetches track metadata (title, artist, album, duration, etc.) for a list of track IDs
/// from the specified music API source.
///
/// # Errors
///
/// * `TrackInfoError::NotFound` - If any track was not found
/// * `TrackInfoError::MusicApi` - If failed to get the track info
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

    Ok(tracks.into_iter().map(Into::into).collect())
}

/// Retrieves metadata information for a single track.
///
/// Fetches track metadata (title, artist, album, duration, etc.) for a track ID from the
/// specified music API source.
///
/// # Errors
///
/// * `TrackInfoError::NotFound` - If the track was not found
/// * `TrackInfoError::MusicApi` - If failed to get the track info
pub async fn get_track_info(
    api: &dyn MusicApi,
    track_id: &Id,
) -> Result<TrackInfo, TrackInfoError> {
    log::debug!("Getting track info {track_id}");

    let track = api.track(track_id).await?;

    log::trace!("Got track {track:?}");

    let Some(track) = track else {
        return Err(TrackInfoError::NotFound(track_id.to_owned()));
    };

    Ok(track.into())
}

const DIV: u16 = u16::MAX / u8::MAX as u16;

/// Generates visualization data from an audio buffer by averaging sample amplitudes.
///
/// Converts audio samples to visual data points by calculating absolute values and averaging
/// across channels. Each output byte represents the amplitude at that point in time.
///
/// # Panics
///
/// * If fails to convert sample into `u16`
#[must_use]
pub fn visualize<S>(input: &AudioBuffer<S>) -> Vec<u8>
where
    S: Sample + IntoSample<i16>,
{
    let channels = input.spec().channels.count();

    let mut values = vec![0; input.capacity()];

    for c in 0..channels {
        for (i, x) in input.chan(c).iter().enumerate() {
            let value = u16::try_from(clamp_i16(i32::from((*x).into_sample()).abs())).unwrap();
            values[i] += (value / DIV) as u8;
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    for value in &mut values {
        *value /= channels as u8;
    }

    values
}

/// Generates or retrieves visualization data for a track.
///
/// Decodes the track audio and generates amplitude data for visualization purposes (waveform).
/// The data is downsampled to the specified maximum number of points.
///
/// # Errors
///
/// * `TrackInfoError::GetTrackBytes` - If failed to get track bytes
/// * `TrackInfoError::Decode` - If audio decoding fails
/// * `TrackInfoError::UnsupportedFormat` - If the audio format is not supported
/// * `TrackInfoError::UnsupportedSource` - If the track source is not supported
///
/// # Panics
///
/// * If the `RwLock` is poisoned
pub async fn get_or_init_track_visualization(
    source: &TrackSource,
    max: u16,
) -> Result<Vec<u8>, TrackInfoError> {
    const MAX_DELTA: i16 = 50;

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
            moosicbox_audio_decoder::AudioDecodeHandler::new().with_filter(Box::new(
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
    let mss = MediaSourceStream::new(Box::new(media_source), MediaSourceStreamOptions::default());

    decode_media_source_async(mss, &hint, get_handler, true, true, None, None).await?;

    let viz = viz.read().unwrap();
    let count = std::cmp::min(max as usize, viz.len());
    let mut ret_viz = Vec::with_capacity(count);

    if viz.len() > max as usize {
        #[allow(clippy::cast_precision_loss)]
        let offset = (viz.len() as f64) / f64::from(max);
        log::debug!("Trimming visualization: offset={offset}");
        let mut last_pos = 0_usize;
        let mut pos = offset;

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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

    drop(viz);

    let mut min_value = u8::MAX;
    let mut max_value = 0;

    for x in &ret_viz {
        let x = *x;

        if x < min_value {
            min_value = x;
        }
        if x > max_value {
            max_value = x;
        }
    }

    let dyn_range = max_value - min_value;
    let coefficient = f64::from(u8::MAX) / f64::from(dyn_range);

    log::debug!(
        "dyn_range={dyn_range} coefficient={coefficient} min_value={min_value} max_value={max_value}"
    );

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for x in &mut ret_viz {
        *x -= min_value;
        let diff = f64::from(*x) * coefficient;
        *x = diff as u8;
    }

    let mut smooth_viz = vec![0; ret_viz.len()];
    let mut last = 0;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for (i, x) in smooth_viz.iter_mut().enumerate() {
        let mut current = i16::from(ret_viz[i]);

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

/// Retrieves the file size of a track at the specified quality.
///
/// Gets the total size in bytes of the track audio data. This is useful for setting
/// Content-Length headers and calculating download progress.
///
/// # Errors
///
/// * `TrackInfoError::NotFound` - If the track was not found
/// * `TrackInfoError::MusicApi` - If failed to get the track size
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "format-aac")]
    #[test_log::test]
    fn test_audio_format_to_content_type_aac() {
        assert_eq!(
            audio_format_to_content_type(&AudioFormat::Aac),
            Some("audio/m4a".to_string())
        );
    }

    #[cfg(feature = "format-flac")]
    #[test_log::test]
    fn test_audio_format_to_content_type_flac() {
        assert_eq!(
            audio_format_to_content_type(&AudioFormat::Flac),
            Some("audio/flac".to_string())
        );
    }

    #[cfg(feature = "format-mp3")]
    #[test_log::test]
    fn test_audio_format_to_content_type_mp3() {
        assert_eq!(
            audio_format_to_content_type(&AudioFormat::Mp3),
            Some("audio/mp3".to_string())
        );
    }

    #[cfg(feature = "format-opus")]
    #[test_log::test]
    fn test_audio_format_to_content_type_opus() {
        assert_eq!(
            audio_format_to_content_type(&AudioFormat::Opus),
            Some("audio/opus".to_string())
        );
    }

    #[test_log::test]
    fn test_audio_format_to_content_type_source() {
        assert_eq!(audio_format_to_content_type(&AudioFormat::Source), None);
    }

    #[test_log::test]
    fn test_visualize_empty_buffer() {
        use symphonia::core::audio::AudioBuffer;

        let spec = symphonia::core::audio::SignalSpec::new(
            44100,
            symphonia::core::audio::Channels::FRONT_LEFT
                | symphonia::core::audio::Channels::FRONT_RIGHT,
        );
        let buffer = AudioBuffer::<i16>::new(0, spec);
        let result = visualize(&buffer);
        assert_eq!(result.len(), 0);
    }

    #[test_log::test]
    fn test_visualize_with_data() {
        use symphonia::core::audio::Signal as _;

        let spec = symphonia::core::audio::SignalSpec::new(
            44100,
            symphonia::core::audio::Channels::FRONT_LEFT
                | symphonia::core::audio::Channels::FRONT_RIGHT,
        );
        let mut buffer = AudioBuffer::<i16>::new(100, spec);

        // Fill with some test data
        for ch in 0..buffer.spec().channels.count() {
            let chan = buffer.chan_mut(ch);
            for (i, sample) in chan.iter_mut().enumerate() {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                {
                    *sample = (i * 100) as i16;
                }
            }
        }

        let result = visualize(&buffer);
        assert_eq!(result.len(), 100);
        // First value should be 0 (average of channel samples which are 0)
        assert_eq!(result[0], 0);
    }
}
