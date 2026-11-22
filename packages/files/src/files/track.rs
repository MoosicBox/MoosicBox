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
    DecodeError, DecodeOptions, decode_file_path_str_async_with_options, decode_media_source_async,
    decode_media_source_async_with_options,
    media_sources::remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_audio_output::{AudioOutputError, AudioSpec, AudioWrite};
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
    audio::{Audio, AudioBuffer, conv::IntoSample, sample::Sample},
    formats::probe::Hint,
    io::{MediaSourceStream, MediaSourceStreamOptions},
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
        track.id,
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
    Acquire(#[from] switchy_async::sync::AcquireError),
    /// Channel receive error
    #[error(transparent)]
    Recv(#[from] RecvError),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Track info retrieval error
    #[error(transparent)]
    TrackInfo(#[from] Box<TrackInfoError>),
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
                    return Err(GetTrackBytesError::TrackInfo(Box::new(err)));
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
    /// The requested silence cannot fit in an audio buffer on this platform.
    #[error("Silence duration exceeds buffer capacity")]
    DurationTooLarge,
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
/// * `GetSilenceBytesError::DurationTooLarge` - If the silence cannot fit in a buffer on this platform
/// * `GetSilenceBytesError::AudioOutput` - If failed to encode the audio bytes
/// * `GetSilenceBytesError::InvalidSource` - If the `AudioFormat` is `Source` (invalid for generated audio)
///
/// # Panics
///
/// * If an encoder feature is not enabled for the `AudioFormat`
#[allow(unused_assignments, clippy::too_many_lines)]
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
    let spec = AudioSpec::new(
        44_100,
        symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
    );
    #[allow(unused)]
    let duration: u64 = u64::from(spec.rate()) * duration;

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
                                .open(spec.clone(), duration.into()),
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
                                .open(spec.clone(), duration.into()),
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
                                .open(spec.clone(), duration.into()),
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
                                .open(spec.clone(), duration.into()),
                        )
                    }
                    #[cfg(all(not(feature = "encoder-opus"), feature = "encoder-opus-native"))]
                    {
                        use moosicbox_audio_output::encoder::opus::{
                            OpusEncodeBackend, OpusEncodeConfig, SelectedOpusEncoder,
                        };
                        Box::new(SelectedOpusEncoder::with_writer(
                            writer,
                            OpusEncodeConfig {
                                backend: OpusEncodeBackend::Native,
                                channels: u8::try_from(spec.channels().count())
                                    .map_err(|_| GetSilenceBytesError::InvalidSource)?,
                                ..OpusEncodeConfig::default()
                            },
                        )?)
                    }
                    #[cfg(not(any(feature = "encoder-opus", feature = "encoder-opus-native")))]
                    panic!("No Opus encoder feature");
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
                let mut buffer = AudioBuffer::<f32>::new(
                    spec,
                    usize::try_from(duration)
                        .map_err(|_| GetSilenceBytesError::DurationTooLarge)?,
                );
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
    let mut options = DecodeOptions::default();
    options.decoder.verify = true;
    get_audio_bytes_impl(
        source,
        format,
        size,
        start,
        end,
        options,
        true,
        #[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
        None,
    )
    .await
}

/// Fetch local or remote audio with explicit decoding options.
///
/// Explicit options bypass the legacy shared encoded-byte cache so backend/configuration
/// selection cannot be satisfied by bytes encoded using a different decoder.
///
/// # Errors
/// * Returns the source, streaming, and encoding errors described by [`get_audio_bytes`].
pub async fn get_audio_bytes_with_options(
    source: TrackSource,
    format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
    options: DecodeOptions,
) -> Result<TrackBytes, GetTrackBytesError> {
    get_audio_bytes_impl(
        source,
        format,
        size,
        start,
        end,
        options,
        false,
        #[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
        None,
    )
    .await
}

/// Transcode explicitly to Ogg Opus with the selected backend and coding mode.
///
/// Unlike passthrough APIs, this always re-encodes, including Opus input, and
/// bypasses the shared byte cache. Decoder selection is independent of encoding.
///
/// # Errors
/// * Returns source/transport errors from [`get_audio_bytes`]; codec, unsupported
///   mode/configuration, and output failures surface as terminal stream errors.
#[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
pub async fn get_audio_bytes_with_opus_encoder(
    source: TrackSource,
    options: DecodeOptions,
    config: moosicbox_audio_output::encoder::opus::OpusEncodeConfig,
) -> Result<TrackBytes, GetTrackBytesError> {
    get_audio_bytes_impl(
        source,
        AudioFormat::Opus,
        None,
        None,
        None,
        options,
        false,
        Some(OpusEncoderSelection::Single(config)),
    )
    .await
}

#[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
#[derive(Clone)]
enum OpusEncoderSelection {
    Single(moosicbox_audio_output::encoder::opus::OpusEncodeConfig),
    #[cfg(feature = "encoder-opus-native")]
    Multistream(moosicbox_audio_output::encoder::opus::OpusMultistreamEncodeConfig),
}

/// Transcode to native multistream Ogg Opus without using the shared byte cache.
///
/// # Errors
/// * Returns source errors; invalid configuration and codec failures are terminal stream errors.
#[cfg(feature = "encoder-opus-native")]
pub async fn get_audio_bytes_with_multistream_encoder(
    source: TrackSource,
    options: DecodeOptions,
    config: moosicbox_audio_output::encoder::opus::OpusMultistreamEncodeConfig,
) -> Result<TrackBytes, GetTrackBytesError> {
    get_audio_bytes_impl(
        source,
        AudioFormat::Opus,
        None,
        None,
        None,
        options,
        false,
        Some(OpusEncoderSelection::Multistream(config)),
    )
    .await
}

// Preserve encoded bytes, then expose decoder completion as a terminal stream error.
// ByteWriter itself cannot carry errors; this channel also detects a dropped worker.
fn decoded_byte_stream(
    stream: impl Stream<Item = BytesStreamItem>,
    completion: flume::Receiver<Result<(), std::io::Error>>,
) -> impl Stream<Item = BytesStreamItem> {
    stream.chain(
        futures::stream::once(async move {
            match completion.recv_async().await {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(Err(error)),
                Err(error) => Some(Err(std::io::Error::other(error))),
            }
        })
        .filter_map(futures::future::ready),
    )
}

#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::match_wildcard_for_single_variants
)]
async fn get_audio_bytes_impl(
    source: TrackSource,
    format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
    options: DecodeOptions,
    cache: bool,
    #[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))] opus_config: Option<
        OpusEncoderSelection,
    >,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!("Getting audio bytes format={format:?} size={size:?} start={start:?} end={end:?}");

    let fetch: super::track_pool::FetchTrackBytesFunc = Box::new({
        let source = source.clone();
        move |start, end, size| {
            let source = source.clone();
            #[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
            let opus_config = opus_config.clone();
            Box::pin(async move {
                log::debug!("get_audio_bytes: cache miss; eagerly fetching audio bytes");
                let writer = ByteWriter::default();
                let writer_id = writer.id;
                let (completion, receiver) = flume::bounded(1);
                #[allow(unused)]
                let stream = decoded_byte_stream(writer.stream(), receiver);
                let same_format = format == AudioFormat::Source || source.format() == format;
                #[cfg(any(feature = "encoder-opus", feature = "encoder-opus-native"))]
                let same_format = same_format && opus_config.is_none();

                let track_bytes = if same_format {
                    match source {
                        TrackSource::LocalFilePath { path, .. } => {
                            request_audio_bytes_from_file(path, format, size, start, end).await?
                        }
                        TrackSource::RemoteUrl { url, headers, .. } => {
                            request_track_bytes_from_url(
                                &url,
                                headers.as_deref(),
                                start,
                                end,
                                format,
                                size,
                            )
                            .await?
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
                                #[cfg(any(
                                    feature = "encoder-opus",
                                    feature = "encoder-opus-native"
                                ))]
                                if let Some(config) = opus_config {
                                    return Ok(moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(Box::new(move |_, _| {
                                        let encoder = match &config {
                                            OpusEncoderSelection::Single(config) => moosicbox_audio_output::encoder::opus::SelectedOpusEncoder::with_writer(writer.clone(), *config),
                                            #[cfg(feature = "encoder-opus-native")]
                                            OpusEncoderSelection::Multistream(config) => moosicbox_audio_output::encoder::opus::SelectedOpusEncoder::with_multistream_writer(writer.clone(), config.clone()),
                                        }.map_err(|error| moosicbox_audio_decoder::AudioDecodeError::Other(Box::new(error)))?;
                                        Ok(Box::new(encoder))
                                    })));
                                }
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
                                #[cfg(all(
                                    not(feature = "encoder-opus"),
                                    feature = "encoder-opus-native"
                                ))]
                                {
                                    moosicbox_audio_decoder::AudioDecodeHandler::new().with_output(Box::new(move |spec, _| {
                                        use moosicbox_audio_output::encoder::opus::{SelectedOpusEncoder, OpusEncodeConfig, OpusEncodeBackend};
                                        let channels = u8::try_from(spec.channels().count()).map_err(|_| moosicbox_audio_decoder::AudioDecodeError::OpenStream)?;
                                        Ok(Box::new(SelectedOpusEncoder::with_writer(writer.clone(), OpusEncodeConfig { backend: OpusEncodeBackend::Native, channels, ..OpusEncodeConfig::default() }).map_err(|error| moosicbox_audio_decoder::AudioDecodeError::Other(Box::new(error)))?))
                                    }))
                                }
                                #[cfg(not(any(
                                    feature = "encoder-opus",
                                    feature = "encoder-opus-native"
                                )))]
                                panic!("No Opus encoder feature");
                            }
                            AudioFormat::Source => {
                                return Err(moosicbox_audio_decoder::DecodeError::InvalidSource);
                            }
                        })
                    };

                    let result = match &source {
                        TrackSource::LocalFilePath { path, .. } => {
                            decode_file_path_str_async_with_options(
                                path,
                                get_handler,
                                options,
                                None,
                                None,
                            )
                            .await
                        }
                        TrackSource::RemoteUrl { url, .. } => {
                            let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
                                url.clone(),
                                size,
                                true,
                                size.is_some(),
                                CancellationToken::new(),
                            )
                            .into();
                            decode_media_source_async_with_options(
                                MediaSourceStream::new(
                                    Box::new(source),
                                    MediaSourceStreamOptions::default(),
                                ),
                                &Hint::new(),
                                get_handler,
                                options,
                                None,
                                None,
                            )
                            .await
                        }
                    };
                    if let Err(err) = &result {
                        log::error!(
                            "Failed to encode to {format} (source={}): {err:?}",
                            source.format()
                        );
                    }
                    let _ = completion.send(result.map(|_| ()).map_err(std::io::Error::other));

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
                                request_track_bytes_from_url(
                                    &url,
                                    headers.as_deref(),
                                    start,
                                    end,
                                    format,
                                    size,
                                )
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
                        },
                    }
                };

                Ok(track_bytes)
            })
        }
    });
    // The legacy byte pool logs upstream errors and cannot replay them to readers.
    // Keep it for passthrough only; transcoding must preserve terminal decode errors.
    if cache && (format == AudioFormat::Source || source.format() == format) {
        get_or_fetch_track(&source, format, size, start, end, fetch).await
    } else {
        fetch(start, end, size).await
    }
}

/// Validate an end-exclusive byte range and return its selected length.
fn selected_byte_length(
    start: Option<u64>,
    end: Option<u64>,
    original_size: Option<u64>,
) -> std::io::Result<Option<u64>> {
    let start = start.unwrap_or(0);
    if end.is_some_and(|end| end < start)
        || original_size.is_some_and(|size| start > size || end.is_some_and(|end| end > size))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "byte range must satisfy 0 <= start <= end <= source size",
        ));
    }
    Ok(end.or(original_size).map(|end| end - start))
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

    let original_size = if let Some(size) = size {
        size
    } else {
        file.metadata().await?.len()
    };

    let size = selected_byte_length(start, end, Some(original_size))?
        .ok_or_else(|| std::io::Error::other("missing local file size"))?;
    if let Some(start) = start {
        file.seek(std::io::SeekFrom::Start(start)).await?;
    }

    log::debug!(
        "request_audio_bytes_from_file calculated size={size} original_size={original_size}"
    );

    // A bounded queue applies backpressure and observes consumer cancellation.
    let (sender, receiver) = flume::bounded(1);

    switchy_async::runtime::Handle::current().spawn_with_name(
        "files: Manual file reader",
        async move {
            // Move the already-opened, already-seeked file into the task. Reopening
            // could race with replacement and previously hid open/seek failures.
            let mut remaining = size;
            let mut buffer = vec![0u8; 8192];
            while remaining > 0 && !sender.is_disconnected() {
                let limit = usize::try_from(remaining.min(buffer.len() as u64)).unwrap();
                match file.read(&mut buffer[..limit]).await {
                    Ok(0) => {
                        let _ = sender
                            .send_async(Err(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "source ended before the selected byte range",
                            )))
                            .await;
                        break;
                    }
                    Ok(count) => {
                        remaining -= count as u64;
                        if sender
                            .send_async(Ok(Bytes::copy_from_slice(&buffer[..count])))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send_async(Err(error)).await;
                        break;
                    }
                }
            }
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
    let _ = selected_byte_length(start, end, size)?;
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

    // HEAD describes the entire object, never the selected Range response.
    let original_size = if let Some(size) = size {
        Some(size)
    } else {
        let mut head = head_request.send().await?;
        head.headers()
            .get(switchy_http::Header::ContentLength.as_ref())
            .map(|value| value.parse::<u64>())
            .transpose()?
    };
    let selected_size = selected_byte_length(start, end, original_size)?;
    if selected_size == Some(0) {
        return Ok(TrackBytes {
            id: new_byte_writer_id(),
            stream: StalledReadMonitor::new(futures::stream::empty().boxed()),
            size: selected_size,
            original_size,
            format,
            filename: None,
        });
    }
    if start.is_some() || end.is_some() {
        // Public byte ranges are end-exclusive; HTTP ranges are inclusive.
        let start = start.unwrap_or(0);
        let last = end.map_or_else(String::new, |end| (end - 1).to_string());
        request = request.header(
            switchy_http::Header::Range.as_ref(),
            &format!("bytes={start}-{last}"),
        );
    }

    log::debug!("request_track_bytes_from_url: Sending request to url={url}");
    let stream = request
        .send()
        .await?
        .bytes_stream()
        .map_err(std::io::Error::other);

    Ok(TrackBytes {
        id: new_byte_writer_id(),
        stream: StalledReadMonitor::new(stream.boxed()),
        size: selected_size,
        original_size,
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
    UnsupportedSource(Box<TrackSource>),
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
    let channels = input.spec().channels().count();

    let mut values = vec![0; input.capacity()];

    for c in 0..channels {
        for (i, x) in input
            .plane(c)
            .expect("valid audio channel")
            .iter()
            .enumerate()
        {
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
    #[test]
    fn selected_byte_ranges_are_end_exclusive_checked_and_allow_empty() {
        for (start, end, original, expected) in [
            (None, None, Some(10), Some(10)),
            (Some(3), Some(7), Some(10), Some(4)),
            (None, Some(7), Some(10), Some(7)),
            (Some(3), None, Some(10), Some(7)),
            (Some(10), None, Some(10), Some(0)),
            (Some(3), Some(3), None, Some(0)),
            (Some(3), None, None, None),
            (None, Some(0), None, Some(0)),
        ] {
            assert_eq!(
                super::selected_byte_length(start, end, original).unwrap(),
                expected
            );
        }
        for (start, end, original) in [
            (Some(8), Some(7), None),
            (Some(11), None, Some(10)),
            (None, Some(11), Some(10)),
            (Some(u64::MAX), Some(0), Some(10)),
        ] {
            assert_eq!(
                super::selected_byte_length(start, end, original)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::InvalidInput
            );
        }
    }

    #[switchy_async::test]
    async fn local_ranges_preserve_size_empty_and_terminal_eof_errors() {
        use futures::StreamExt;
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/silk.ogg");
        let expected = std::fs::read(path).unwrap();
        for (start, end) in [
            (Some(2), Some(9)),
            (None, Some(0)),
            (Some(expected.len() as u64), None),
        ] {
            let track = super::request_audio_bytes_from_file(
                path.to_owned(),
                super::AudioFormat::Source,
                None,
                start,
                end,
            )
            .await
            .unwrap();
            assert_eq!(track.original_size, Some(expected.len() as u64));
            let first = usize::try_from(start.unwrap_or(0)).unwrap();
            let last = usize::try_from(end.unwrap_or(expected.len() as u64)).unwrap();
            assert_eq!(track.size, Some((last - first) as u64));
            let mut actual = Vec::new();
            for item in track.stream.collect::<Vec<_>>().await {
                actual.extend_from_slice(&item.unwrap().unwrap());
            }
            assert_eq!(actual, expected[first..last]);
        }
        assert!(
            matches!(super::request_audio_bytes_from_file(path.to_owned(), super::AudioFormat::Source, None, Some(9), Some(2)).await, Err(error) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
        // Simulate a source shorter than its supplied metadata without mutating fixtures.
        let track = super::request_audio_bytes_from_file(
            path.to_owned(),
            super::AudioFormat::Source,
            Some(expected.len() as u64 + 1),
            None,
            None,
        )
        .await
        .unwrap();
        let items: Vec<_> = track.stream.collect().await;
        assert!(
            matches!(items.last().unwrap(), Ok(Err(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }

    #[switchy_async::test]
    async fn remote_empty_and_invalid_ranges_do_not_issue_get() {
        // An unreachable URL proves validated known-size empty ranges need no request.
        let empty = super::request_track_bytes_from_url(
            "http://127.0.0.1:1/unreachable",
            None,
            Some(10),
            Some(10),
            super::AudioFormat::Source,
            Some(10),
        )
        .await
        .unwrap();
        assert_eq!(empty.size, Some(0));
        assert_eq!(empty.original_size, Some(10));
        assert!(empty.stream.collect::<Vec<_>>().await.is_empty());
        let error = super::request_track_bytes_from_url(
            "http://127.0.0.1:1/unreachable",
            None,
            Some(11),
            None,
            super::AudioFormat::Source,
            Some(10),
        )
        .await;
        assert!(
            matches!(error, Err(super::GetTrackBytesError::IO(error)) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
    }

    #[test]
    fn remote_ranges_use_full_head_size_and_inclusive_http_end() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(remote_range_http_case());
    }

    async fn remote_range_http_case() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/audio", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut requests = Vec::new();
            while requests.len() < 2 && std::time::Instant::now() < deadline {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                if request.starts_with("head ") {
                    assert!(!request.contains("\r\nrange:"));
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                } else {
                    assert!(request.contains("\r\nrange: bytes=2-5\r\n"));
                    socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 2-5/10\r\nConnection: close\r\n\r\n2345").unwrap();
                }
                requests.push(request);
            }
            assert_eq!(requests.len(), 2);
        });
        let track = super::request_track_bytes_from_url(
            &url,
            None,
            Some(2),
            Some(6),
            super::AudioFormat::Source,
            None,
        )
        .await
        .unwrap();
        assert_eq!(track.original_size, Some(10));
        assert_eq!(track.size, Some(4));
        let mut actual = Vec::new();
        for item in track.stream.collect::<Vec<_>>().await {
            actual.extend_from_slice(&item.unwrap().unwrap());
        }
        assert_eq!(actual, b"2345");
        server.join().unwrap();
    }

    #[cfg(feature = "encoder-opus-native")]
    #[switchy_async::test]
    #[allow(clippy::default_trait_access)]
    async fn explicit_opus_modes_transcode_opus_source_instead_of_passthrough() {
        use moosicbox_audio_output::encoder::opus::{
            OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode,
        };
        use moosicbox_music_models::{ApiSource, TrackApiSource};
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Opus,
            path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/silk.ogg").to_owned(),
            track_id: None,
            source: TrackApiSource::Api(ApiSource::library()),
        };
        for (mode, rate) in [
            (OpusEncodeMode::Celt, 48000),
            (OpusEncodeMode::Silk, 16000),
            (OpusEncodeMode::Hybrid, 48000),
        ] {
            let result = get_audio_bytes_with_opus_encoder(
                source.clone(),
                DecodeOptions {
                    opus_backend: moosicbox_audio_decoder::OpusDecodeBackend::Native,
                    ..Default::default()
                },
                OpusEncodeConfig {
                    backend: OpusEncodeBackend::Native,
                    mode,
                    sample_rate: rate,
                    channels: 1,
                    frame_size: rate as usize / 50,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let mut bytes = Vec::new();
            for item in result.stream.collect::<Vec<_>>().await {
                bytes.extend_from_slice(&item.unwrap().unwrap());
            }
            let mut format = moosicbox_audio_decoder::create_format_probe()
                .probe(
                    &symphonia::core::formats::probe::Hint::new(),
                    symphonia::core::io::MediaSourceStream::new(
                        Box::new(std::io::Cursor::new(bytes)),
                        Default::default(),
                    ),
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
            let packet = format.next_packet().unwrap().unwrap();
            let toc = packet.data[0] >> 3;
            match mode {
                OpusEncodeMode::Celt => assert!(toc >= 16),
                OpusEncodeMode::Silk => assert!(toc < 12),
                OpusEncodeMode::Hybrid => assert!((12..16).contains(&toc)),
                OpusEncodeMode::Default | OpusEncodeMode::Auto => unreachable!(),
            }
        }
    }
    #[cfg(all(
        feature = "encoder-flac",
        feature = "decoder-opus-native",
        not(feature = "decoder-opus")
    ))]
    #[switchy_async::test]
    async fn public_file_transcode_reports_unavailable_backend_in_stream() {
        use moosicbox_music_models::{ApiSource, TrackApiSource};
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Opus,
            path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/silk.ogg").to_owned(),
            track_id: None,
            source: TrackApiSource::Api(ApiSource::library()),
        };
        // Compatibility defaults request Libopus, unavailable in this native-only build.
        // This exercises the legacy API as well as explicit options (neither may hide errors).
        let bytes = get_audio_bytes(source.clone(), AudioFormat::Flac, None, None, None)
            .await
            .unwrap();
        let items: Vec<_> = bytes.stream.collect().await;
        assert_eq!(items.len(), 1);
        assert!(
            items[0]
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .to_string()
                .contains("backend is not enabled")
        );
        let bytes = get_audio_bytes_with_options(
            source,
            AudioFormat::Flac,
            None,
            None,
            None,
            DecodeOptions::default(),
        )
        .await
        .unwrap();
        let items: Vec<_> = bytes.stream.collect().await;
        assert_eq!(items.len(), 1);
        assert!(items[0].as_ref().unwrap().is_err());
    }

    #[switchy_async::test]
    async fn decoded_bytes_surface_worker_failure_after_partial_output() {
        let mut writer = ByteWriter::default();
        let (sender, receiver) = flume::bounded(1);
        let stream = decoded_byte_stream(writer.stream(), receiver);
        std::io::Write::write_all(&mut writer, b"partial").unwrap();
        drop(writer);
        sender
            .send(Err(std::io::Error::other(
                "native configuration unsupported",
            )))
            .unwrap();
        let items: Vec<_> = stream.collect().await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_ref().unwrap().as_ref(), b"partial");
        assert_eq!(
            items[1].as_ref().unwrap_err().to_string(),
            "native configuration unsupported"
        );
    }
    #[switchy_async::test]
    async fn decoded_bytes_success_and_dropped_worker_are_distinct() {
        for success in [true, false] {
            let writer = ByteWriter::default();
            let (sender, receiver) = flume::bounded(1);
            let stream = decoded_byte_stream(writer.stream(), receiver);
            drop(writer);
            if success {
                sender.send(Ok(())).unwrap();
            }
            drop(sender);
            let items: Vec<_> = stream.collect().await;
            if success {
                assert!(items.is_empty());
            } else {
                assert_eq!(items.len(), 1);
                assert!(items[0].is_err());
            }
        }
    }

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
        {
            use symphonia::core::audio::AudioBuffer;

            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let buffer = AudioBuffer::<i16>::new(spec, 0);
            let result = visualize(&buffer);
            assert_eq!(result.len(), 0);
        }
    }

    #[test_log::test]
    fn test_visualize_with_data() {
        {
            use symphonia::core::audio::AudioMut as _;

            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 100);

            // Fill with some test data
            for ch in 0..buffer.spec().channels().count() {
                let chan = buffer.plane_mut(ch).expect("valid audio channel");
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

    #[test_log::test]
    fn test_visualize_mono_audio() {
        {
            use symphonia::core::audio::AudioMut as _;

            // Test with mono (single channel) audio
            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::Channels::from(
                    symphonia::core::audio::Position::FRONT_LEFT,
                ),
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 50);

            // Render silence first to materialize samples in the buffer
            buffer.render_silence(None);

            // Now modify the samples
            let chan = buffer.plane_mut(0).expect("valid audio channel");
            for (i, sample) in chan.iter_mut().enumerate() {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                {
                    // Values that will produce non-zero visualization after DIV scaling
                    // DIV = 257, so values >= 257 should produce at least 1
                    *sample = ((i + 1) * 300) as i16;
                }
            }

            let result = visualize(&buffer);
            assert_eq!(result.len(), 50);
            // First sample: 300 / DIV = 300 / 257 = 1
            assert_eq!(result[0], 1);
            // Second sample: 600 / DIV = 600 / 257 = 2
            assert_eq!(result[1], 2);
        }
    }

    #[test_log::test]
    fn test_visualize_negative_values() {
        {
            use symphonia::core::audio::AudioMut as _;

            // Test that negative values are correctly converted to positive via abs()
            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 10);

            // Render silence first to materialize samples in the buffer
            buffer.render_silence(None);

            // Fill channel 0 with negative values
            let chan0 = buffer.plane_mut(0).expect("valid audio channel");
            for sample in chan0.iter_mut() {
                *sample = -1000; // Negative value
            }

            // Fill channel 1 with same positive values
            let chan1 = buffer.plane_mut(1).expect("valid audio channel");
            for sample in chan1.iter_mut() {
                *sample = 1000; // Positive value
            }

            let result = visualize(&buffer);
            assert_eq!(result.len(), 10);
            // Both channels contribute 1000, after abs() both are 1000
            // 1000 / DIV = 1000 / 257 = 3 per channel
            // Average of 2 channels: (3 + 3) / 2 = 3
            for value in &result {
                assert_eq!(*value, 3);
            }
        }
    }

    #[test_log::test]
    fn test_visualize_asymmetric_channels() {
        {
            use symphonia::core::audio::AudioMut as _;

            // Test averaging with different values per channel
            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 5);

            // Render silence first to materialize samples in the buffer
            buffer.render_silence(None);

            // Channel 0: high amplitude
            let chan0 = buffer.plane_mut(0).expect("valid audio channel");
            for sample in chan0.iter_mut() {
                *sample = 2570; // 2570 / DIV = 10
            }

            // Channel 1: low amplitude
            let chan1 = buffer.plane_mut(1).expect("valid audio channel");
            for sample in chan1.iter_mut() {
                *sample = 0;
            }

            let result = visualize(&buffer);
            assert_eq!(result.len(), 5);
            // Channel 0 contributes 10, channel 1 contributes 0
            // Average: (10 + 0) / 2 = 5
            for value in &result {
                assert_eq!(*value, 5);
            }
        }
    }

    #[test_log::test]
    fn test_visualize_maximum_amplitude() {
        {
            use symphonia::core::audio::AudioMut as _;

            // Test with maximum positive amplitude (i16::MAX = 32767)
            // Verifies no overflow when handling maximum sample values
            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 5);
            buffer.render_silence(None);

            // Fill both channels with maximum amplitude
            for ch in 0..2 {
                let chan = buffer.plane_mut(ch).expect("valid audio channel");
                for sample in chan.iter_mut() {
                    *sample = i16::MAX; // 32767
                }
            }

            let result = visualize(&buffer);
            assert_eq!(result.len(), 5);
            // i16::MAX (32767) / DIV (257) = 127 per channel
            // Both channels contribute 127, average = (127 + 127) / 2 = 127
            for value in &result {
                assert_eq!(*value, 127);
            }
        }
    }

    #[test_log::test]
    fn test_visualize_minimum_amplitude() {
        {
            use symphonia::core::audio::AudioMut as _;

            // Test with minimum negative amplitude (i16::MIN = -32768)
            // Verifies the clamping behavior for values that would overflow when negated
            let spec = symphonia::core::audio::AudioSpec::new(
                44100,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut buffer = AudioBuffer::<i16>::new(spec, 5);
            buffer.render_silence(None);

            // Fill both channels with minimum amplitude
            for ch in 0..2 {
                let chan = buffer.plane_mut(ch).expect("valid audio channel");
                for sample in chan.iter_mut() {
                    *sample = i16::MIN; // -32768
                }
            }

            let result = visualize(&buffer);
            assert_eq!(result.len(), 5);
            // i16::MIN (-32768) -> abs() in i32 = 32768 -> clamped to i16::MAX (32767)
            // 32767 / DIV (257) = 127 per channel
            // Both channels contribute 127, average = (127 + 127) / 2 = 127
            for value in &result {
                assert_eq!(*value, 127);
            }
        }
    }
}
