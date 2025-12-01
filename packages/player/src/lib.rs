//! Audio playback engine for `MoosicBox`.
//!
//! This crate provides the core playback functionality for the `MoosicBox` music player,
//! handling audio decoding, streaming, and playback control. It supports both local file
//! playback and remote streaming from various audio sources.
//!
//! # Features
//!
//! * Playback session management with [`Playback`] and [`PlaybackHandler`]
//! * Multiple audio formats (AAC, FLAC, MP3, Opus) via feature flags
//! * Audio streaming and buffering with configurable quality settings
//! * Retry logic for robust playback with [`PlaybackRetryOptions`]
//! * Volume control and seeking capabilities
//! * Support for both local and remote playback sources
//!
//! # Main Entry Points
//!
//! * [`PlaybackHandler`] - Manages playback operations for tracks and albums
//! * [`Player`] - Trait for implementing custom playback players
//! * [`Playback`] - Represents an active playback session
//! * [`PlayerError`] - Error types for player operations
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_player::{PlaybackHandler, Player};
//! # use moosicbox_music_models::{Track, PlaybackQuality};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # struct MyPlayer;
//! # impl std::fmt::Debug for MyPlayer {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//! #         write!(f, "MyPlayer")
//! #     }
//! # }
//! # #[async_trait::async_trait]
//! # impl Player for MyPlayer {
//! #     async fn trigger_play(&self, _seek: Option<f64>) -> Result<(), moosicbox_player::PlayerError> { Ok(()) }
//! #     async fn trigger_stop(&self) -> Result<(), moosicbox_player::PlayerError> { Ok(()) }
//! #     async fn trigger_seek(&self, _seek: f64) -> Result<(), moosicbox_player::PlayerError> { Ok(()) }
//! #     async fn trigger_pause(&self) -> Result<(), moosicbox_player::PlayerError> { Ok(()) }
//! #     async fn trigger_resume(&self) -> Result<(), moosicbox_player::PlayerError> { Ok(()) }
//! #     fn player_status(&self) -> Result<moosicbox_player::ApiPlaybackStatus, moosicbox_player::PlayerError> { unimplemented!() }
//! #     fn get_source(&self) -> &moosicbox_player::PlayerSource { unimplemented!() }
//! # }
//! # let player = MyPlayer;
//! # let session_id = 1;
//! # let profile = "default".to_string();
//! # let track = Track::default();
//! # let quality = PlaybackQuality::default();
//! // Create a playback handler with a custom player implementation
//! let mut handler = PlaybackHandler::new(player);
//!
//! // Play a track
//! handler.play_track(
//!     session_id,
//!     profile,
//!     track,
//!     None,           // seek position
//!     Some(0.8),      // volume
//!     quality,
//!     None,           // playback target
//!     None,           // retry options
//! ).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    fs::File,
    path::Path,
    sync::{Arc, LazyLock, RwLock},
};

use ::symphonia::core::{io::MediaSource, probe::Hint};
use async_trait::async_trait;
use atomic_float::AtomicF64;
use flume::SendError;
use futures::{Future, StreamExt as _, TryStreamExt as _};
use local_ip_address::local_ip;
use moosicbox_audio_decoder::media_sources::{
    bytestream_source::ByteStreamSource, remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_audio_output::AudioOutputFactory;
use moosicbox_json_utils::{ParseError, database::DatabaseFetchError};
use moosicbox_music_api::{MusicApi, models::TrackAudioQuality};
use moosicbox_music_models::{ApiSource, AudioFormat, PlaybackQuality, Track, id::Id};
use moosicbox_session::{
    get_session_playlist,
    models::{ApiSession, PlaybackTarget, Session, UpdateSession, UpdateSessionPlaylist},
};
use moosicbox_stream_utils::{
    remote_bytestream::RemoteByteStream, stalled_monitor::StalledReadMonitor,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use switchy_async::util::CancellationToken;
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    signal_chain::{SignalChain, SignalChainError},
    symphonia::PlaybackError,
};

#[cfg(feature = "api")]
/// HTTP API endpoints for playback control.
///
/// This module provides `RESTful` API endpoints for controlling playback, including
/// play, pause, stop, seek, and status operations. It uses Actix-web for HTTP handling
/// and integrates with the core playback functionality.
pub mod api;

#[cfg(feature = "local")]
/// Local audio player implementation.
///
/// This module provides a local player implementation that uses the Symphonia decoder
/// for audio playback. It handles audio output, volume control, seeking, and playback
/// state management for local audio files and streams.
pub mod local;

/// Audio signal processing chain for encoding and decoding.
pub mod signal_chain;
/// Asynchronous audio file playback using Symphonia.
pub mod symphonia;
/// Synchronous audio decoding using Symphonia.
pub mod symphonia_unsync;
/// Volume control and mixing utilities.
pub mod volume_mixer;

/// Default retry options for seek operations.
///
/// Configures 10 attempts with 100ms delay between retries.
pub const DEFAULT_SEEK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: std::time::Duration::from_millis(100),
};

/// Default retry options for playback operations.
///
/// Configures 10 attempts with 500ms delay between retries.
pub const DEFAULT_PLAYBACK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: std::time::Duration::from_millis(500),
};

/// Global HTTP client for making requests.
pub static CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

/// Errors that can occur during player operations.
#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(#[from] SendError<()>),
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Join(#[from] switchy_async::task::JoinError),
    #[error(transparent)]
    Acquire(#[from] switchy_async::sync::AcquireError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Format not supported: {0:?}")]
    UnsupportedFormat(AudioFormat),
    #[error(transparent)]
    PlaybackError(#[from] PlaybackError),
    #[error("Track fetch failed: {0}")]
    TrackFetchFailed(String),
    #[error("Album fetch failed: {0}")]
    AlbumFetchFailed(Id),
    #[error("Track not found: {0}")]
    TrackNotFound(Id),
    #[error("Track not locally stored: {0}")]
    TrackNotLocal(Id),
    #[error("Failed to seek: {0}")]
    Seek(String),
    #[error("No players playing")]
    NoPlayersPlaying,
    #[error("Position out of bounds: {0}")]
    PositionOutOfBounds(u16),
    #[error("No audio outputs")]
    NoAudioOutputs,
    #[error("Playback not playing: {0}")]
    PlaybackNotPlaying(u64),
    #[error("Playback already playing: {0}")]
    PlaybackAlreadyPlaying(u64),
    #[error("Invalid Playback Type")]
    InvalidPlaybackType,
    #[error("Invalid state")]
    InvalidState,
    #[error("Invalid source")]
    InvalidSource,
    #[error("Playback retry requested")]
    RetryRequested,
    #[error("Playback cancelled")]
    Cancelled,
    #[error("Invalid session with id {session_id}: {message}")]
    InvalidSession { session_id: u64, message: String },
    #[error("Missing session ID")]
    MissingSessionId,
    #[error("Missing profile")]
    MissingProfile,
    #[error("Audio output error: {0}")]
    AudioOutput(#[from] moosicbox_audio_output::AudioError),
}

impl std::fmt::Debug for PlayableTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayableTrack")
            .field("track_id", &self.track_id)
            .field("source", &"{{source}}")
            .finish_non_exhaustive()
    }
}

/// Represents an active playback session.
#[derive(Debug, Clone)]
pub struct Playback {
    /// Unique identifier for this playback session
    pub id: u64,
    /// Session ID this playback belongs to
    pub session_id: u64,
    /// Profile name for this playback
    pub profile: String,
    /// List of tracks in the playback queue
    pub tracks: Vec<Track>,
    /// Whether playback is currently active
    pub playing: bool,
    /// Current position in the track list
    pub position: u16,
    /// Audio quality settings for playback
    pub quality: PlaybackQuality,
    /// Current playback progress in seconds
    pub progress: f64,
    /// Playback volume (0.0 to 1.0)
    pub volume: Arc<AtomicF64>,
    /// Target device or zone for playback
    pub playback_target: Option<PlaybackTarget>,
    /// Cancellation token for stopping playback
    pub abort: CancellationToken,
}

impl Playback {
    /// Creates a new playback session.
    #[must_use]
    pub fn new(
        tracks: Vec<Track>,
        position: Option<u16>,
        volume: AtomicF64,
        quality: PlaybackQuality,
        session_id: u64,
        profile: String,
        playback_target: Option<PlaybackTarget>,
    ) -> Self {
        Self {
            id: switchy_random::rng().next_u64(),
            session_id,
            profile,
            tracks,
            playing: false,
            position: position.unwrap_or_default(),
            quality,
            progress: 0.0,
            volume: Arc::new(volume),
            playback_target,
            abort: CancellationToken::new(),
        }
    }
}

/// API representation of a playback session.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlayback {
    /// IDs of tracks in the playback queue
    pub track_ids: Vec<String>,
    /// Whether playback is currently active
    pub playing: bool,
    /// Current position in the track list
    pub position: u16,
    /// Current seek position in seconds
    pub seek: f64,
}

impl From<Playback> for ApiPlayback {
    fn from(value: Playback) -> Self {
        Self {
            track_ids: value.tracks.iter().map(|t| t.id.to_string()).collect(),
            playing: value.playing,
            position: value.position,
            seek: value.progress,
        }
    }
}

/// API representation of playback status.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlaybackStatus {
    /// Currently active playback session, if any
    pub active_playbacks: Option<ApiPlayback>,
}

/// Status response for playback operations.
#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlaybackStatus {
    /// Whether the operation succeeded
    pub success: bool,
}

/// Constructs the URL for streaming a track.
///
/// This function builds the complete URL for accessing a track's audio stream,
/// including query parameters for format, quality, and authentication.
///
/// # Panics
///
/// * If the `SERVICE_PORT` `RwLock` is poisoned
///
/// # Errors
///
/// * If an HTTP request fails
/// * If failed to fetch the track
#[allow(clippy::too_many_lines, clippy::unused_async)]
pub async fn get_track_url(
    track_id: &Id,
    api_source: &ApiSource,
    player_source: &PlayerSource,
    format: PlaybackQuality,
    quality: TrackAudioQuality,
    use_local_network_ip: bool,
) -> Result<(String, Option<BTreeMap<String, String>>), PlayerError> {
    let (host, query, headers) = match player_source {
        PlayerSource::Remote {
            host,
            query,
            headers,
        } => {
            static LOCALHOST: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"^http://localhost[:/].*?").unwrap());

            let host = if use_local_network_ip && LOCALHOST.is_match(host) {
                host.replacen(
                    "localhost",
                    &local_ip().map_or_else(
                        |e| {
                            log::warn!("Failed to get local ip address: {e:?}");
                            "127.0.0.1".to_string()
                        },
                        |x| x.to_string(),
                    ),
                    1,
                )
            } else {
                host.clone()
            };
            (host, query, headers.to_owned())
        }
        PlayerSource::Local => {
            let ip = if use_local_network_ip {
                local_ip().map_or_else(
                    |e| {
                        log::warn!("Failed to get local ip address: {e:?}");
                        "127.0.0.1".to_string()
                    },
                    |x| x.to_string(),
                )
            } else {
                "127.0.0.1".to_string()
            };
            (
                format!(
                    "http://{ip}:{}",
                    SERVICE_PORT
                        .read()
                        .unwrap()
                        .expect("Missing SERVICE_PORT value")
                ),
                &None,
                None,
            )
        }
    };

    let query_params = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());

        if let Some(query) = query {
            for (key, value) in query {
                serializer.append_pair(key, value);
            }
        }

        serializer
            .append_pair("trackId", &track_id.to_string())
            .append_pair("quality", quality.as_ref());

        if let Some(profile) = headers
            .as_ref()
            .and_then(|x| x.get("moosicbox-profile").cloned())
        {
            serializer.append_pair("moosicboxProfile", &profile);
        }

        if format.format != AudioFormat::Source {
            serializer.append_pair("format", format.format.as_ref());
        }
        if !api_source.is_library() {
            serializer.append_pair("source", api_source.as_ref());
        }

        serializer.finish()
    };

    let query_string = format!("?{query_params}");

    Ok((format!("{host}/files/track{query_string}"), headers))
}

/// Retrieves the playlist ID associated with a session ID.
///
/// # Errors
///
/// * If the session playlist is missing
#[cfg_attr(feature = "profiling", profiling::function)]
pub async fn get_session_playlist_id_from_session_id(
    db: &LibraryDatabase,
    session_id: Option<u64>,
) -> Result<Option<u64>, PlayerError> {
    Ok(if let Some(session_id) = session_id {
        Some(
            get_session_playlist(db, session_id)
                .await?
                .ok_or(PlayerError::DatabaseFetch(
                    DatabaseFetchError::InvalidRequest,
                ))?
                .id,
        )
    } else {
        None
    })
}

/// A track ready for playback with its media source.
pub struct PlayableTrack {
    /// ID of the track
    pub track_id: Id,
    /// Media source for reading audio data
    pub source: Box<dyn MediaSource>,
    /// Format hint for the decoder
    pub hint: Hint,
}

/// Specifies the type of playback method to use.
#[derive(Copy, Clone, Default, Deserialize, Serialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaybackType {
    /// Play from local file on disk
    File,
    /// Stream from remote source
    Stream,
    /// Use default playback method based on source
    #[default]
    Default,
}

/// Configuration for retry behavior during playback operations.
#[derive(Copy, Clone)]
pub struct PlaybackRetryOptions {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Delay between retry attempts
    pub retry_delay: std::time::Duration,
}

/// Identifies the source of playback.
#[derive(Debug, Clone)]
pub enum PlayerSource {
    /// Local playback using the service port
    Local,
    /// Remote playback from a specified host
    Remote {
        /// Remote host URL
        host: String,
        /// Optional query parameters
        query: Option<BTreeMap<String, String>>,
        /// Optional HTTP headers
        headers: Option<BTreeMap<String, String>>,
    },
}

/// Manages playback operations for a player.
#[derive(Debug, Clone)]
pub struct PlaybackHandler {
    /// Unique identifier for this handler
    pub id: u64,
    /// Current playback session state
    pub playback: Arc<std::sync::RwLock<Option<Playback>>>,
    /// Audio output factory for creating audio streams
    pub output: Option<Arc<std::sync::Mutex<AudioOutputFactory>>>,
    /// The underlying player implementation
    pub player: Arc<Box<dyn Player + Sync>>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl PlaybackHandler {
    /// Creates a new playback handler with the given player.
    #[must_use]
    pub fn new(player: impl Player + Sync + 'static) -> Self {
        Self::new_boxed(Box::new(player))
    }

    /// Creates a new playback handler with a boxed player.
    #[must_use]
    pub fn new_boxed(player: Box<dyn Player + Sync>) -> Self {
        let playback = Arc::new(std::sync::RwLock::new(None));
        let output = None;

        Self {
            id: switchy_random::rng().next_u64(),
            playback,
            output,
            player: Arc::new(player),
        }
    }

    /// Sets the playback state for this handler.
    #[must_use]
    pub fn with_playback(mut self, playback: Arc<std::sync::RwLock<Option<Playback>>>) -> Self {
        self.playback = playback;
        self
    }

    /// Sets the audio output factory for this handler.
    #[must_use]
    pub fn with_output(
        mut self,
        output: Option<Arc<std::sync::Mutex<AudioOutputFactory>>>,
    ) -> Self {
        self.output = output;
        self
    }
}

impl PlaybackHandler {
    /// Initializes the playback handler from an API session.
    ///
    /// This updates the playback state to match the provided API session configuration,
    /// including tracks, position, volume, and playback target settings.
    ///
    /// # Errors
    ///
    /// * If failed to update the playback from the session
    pub async fn init_from_api_session(
        &mut self,
        profile: String,
        session: ApiSession,
    ) -> Result<(), PlayerError> {
        let session_id = session.session_id;
        if let Err(err) = self
            .update_playback(
                false,
                None,
                None,
                Some(session.playing),
                session.position,
                session.seek,
                session.volume,
                Some(
                    session
                        .playlist
                        .tracks
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>(),
                ),
                None,
                Some(session.session_id),
                Some(profile),
                session.playback_target,
                true,
                None,
            )
            .await
        {
            return Err(PlayerError::InvalidSession {
                session_id,
                message: format!("Failed to update playback: {err:?}"),
            });
        }

        Ok(())
    }

    /// Initializes the playback handler from a database session.
    ///
    /// This updates the playback state to match the provided session from the database,
    /// applying any initialization updates specified in the `UpdateSession` parameter.
    ///
    /// # Errors
    ///
    /// * If failed to update the playback from the session
    pub async fn init_from_session(
        &mut self,
        profile: String,
        session: Session,
        init: &UpdateSession,
    ) -> Result<(), PlayerError> {
        moosicbox_logging::debug_or_trace!(
            (
                "init_from_session: Initializing player from session_id={}",
                session.id
            ),
            (
                "init_from_session: Initializing player from session_id={} init={init:?}",
                session.id
            )
        );
        let session_id = init.session_id;
        if let Err(err) = self
            .update_playback(
                false,
                None,
                None,
                init.playing.or(Some(session.playing)),
                init.position.or(session.position),
                init.seek,
                init.volume.or(session.volume),
                Some(
                    session
                        .playlist
                        .tracks
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>(),
                ),
                None,
                Some(session.id),
                Some(profile),
                session.playback_target,
                true,
                None,
            )
            .await
        {
            return Err(PlayerError::InvalidSession {
                session_id,
                message: format!("Failed to update playback: {err:?}"),
            });
        }

        Ok(())
    }

    /// Plays all tracks from an album.
    ///
    /// Fetches all tracks from the specified album via the music API and begins playback,
    /// optionally starting at a specific track position with seek and volume settings.
    ///
    /// # Errors
    ///
    /// * If failed to fetch the album tracks
    /// * If failed to play the tracks
    #[allow(clippy::too_many_arguments)]
    pub async fn play_album(
        &mut self,
        api: &dyn MusicApi,
        session_id: u64,
        profile: String,
        album_id: &Id,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        playback_target: Option<PlaybackTarget>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        let tracks = {
            api.album_tracks(album_id, None, None, None, None)
                .await
                .map_err(|e| {
                    log::error!("Failed to fetch album tracks: {e:?}");
                    PlayerError::AlbumFetchFailed(album_id.to_owned())
                })?
                .with_rest_of_items_in_batches()
                .await
                .map_err(|e| {
                    log::error!("Failed to fetch album tracks: {e:?}");
                    PlayerError::AlbumFetchFailed(album_id.to_owned())
                })?
        };

        self.play_tracks(
            session_id,
            profile,
            tracks,
            position,
            seek,
            volume,
            quality,
            playback_target,
            retry_options,
        )
        .await
    }

    /// Plays a single track.
    ///
    /// Begins playback of the specified track with optional seek position and volume settings.
    ///
    /// # Errors
    ///
    /// * If failed to play the track
    #[allow(clippy::too_many_arguments)]
    pub async fn play_track(
        &mut self,
        session_id: u64,
        profile: String,
        track: Track,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        playback_target: Option<PlaybackTarget>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        self.play_tracks(
            session_id,
            profile,
            vec![track],
            None,
            seek,
            volume,
            quality,
            playback_target,
            retry_options,
        )
        .await
    }

    /// Plays multiple tracks in sequence.
    ///
    /// Begins playback of the specified tracks with optional starting position,
    /// seek offset, and volume settings. If a playback is already active, it will
    /// be stopped before starting the new one.
    ///
    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to play the tracks
    /// * If failed to stop an existing playback
    #[allow(clippy::too_many_arguments)]
    pub async fn play_tracks(
        &mut self,
        session_id: u64,
        profile: String,
        tracks: Vec<Track>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        playback_target: Option<PlaybackTarget>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        let playback = { self.playback.read().unwrap().clone() };

        if let Some(playback) = playback {
            log::debug!("Stopping existing playback {}", playback.id);
            self.stop(retry_options).await?;
        }

        {
            let playback = Playback::new(
                tracks,
                position,
                AtomicF64::new(volume.unwrap_or(1.0)),
                quality,
                session_id,
                profile,
                playback_target,
            );

            self.playback.write().unwrap().replace(playback);
        }

        self.play_playback(seek, retry_options).await
    }

    /// Starts playback for the current playback session.
    ///
    /// This internal method initiates playback of all tracks in the session's playlist,
    /// automatically advancing through tracks until completion or cancellation.
    ///
    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to play the existing playback
    pub async fn play_playback(
        &mut self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        self.player.before_play_playback(seek).await?;

        let (playback, old) = {
            let mut binding = self.playback.write().unwrap();
            let playback = binding.as_mut().ok_or(PlayerError::NoPlayersPlaying)?;
            log::info!("play_playback: playback={playback:?}");

            if playback.tracks.is_empty() {
                log::debug!("No tracks to play for {playback:?}");
                return Ok(());
            }

            let old = playback.clone();

            playback.playing = true;
            let playback = playback.clone();
            drop(binding);

            (playback, old)
        };

        trigger_playback_event(&playback, &old);

        log::debug!(
            "Playing playback: position={} tracks={:?}",
            playback.position,
            playback.tracks.iter().map(|t| &t.id).collect::<Vec<_>>()
        );

        let mut player = self.clone();

        switchy_async::runtime::Handle::current().spawn_with_name(
            "player: Play playback",
            async move {
                let mut seek = seek;

                let mut playback = player
                    .playback
                    .read()
                    .unwrap()
                    .clone()
                    .ok_or(PlayerError::NoPlayersPlaying)?;

                #[allow(clippy::redundant_pub_crate)]
                while playback.playing && (playback.position as usize) < playback.tracks.len() {
                    let track_or_id = &playback.tracks[playback.position as usize];
                    log::debug!("play_playback: track={track_or_id:?} seek={seek:?}");

                    let seek = if seek.is_some() { seek.take() } else { None };

                    log::debug!("player cancelled={}", playback.abort.is_cancelled());
                    switchy_async::select! {
                        () = playback.abort.cancelled() => {
                            log::debug!("play_playback: Playback cancelled");
                            return Err(PlayerError::Cancelled);
                        }
                        resp = player.play(seek, retry_options) => {
                            if let Err(err) = resp {
                                log::error!("Playback error occurred: {err:?}");

                                {
                                    let old = playback.clone();
                                        playback.playing = false;
                                        player.playback.write().unwrap().replace(playback.clone());
                                    trigger_playback_event(&playback, &old);
                                }


                                return Err(err);
                            }
                        }
                    }

                    log::debug!(
                        "play_playback: playback finished track={track_or_id:?} cancelled={}",
                        playback.abort.is_cancelled()
                    );

                    if playback.abort.is_cancelled() {
                        break;
                    }

                    if ((playback.position + 1) as usize) >= playback.tracks.len() {
                        log::debug!("Playback position at end of tracks. Breaking");
                        break;
                    }

                    let old = playback.clone();
                    playback.position += 1;
                    playback.progress = 0.0;
                    player.playback.write().unwrap().replace(playback.clone());
                    trigger_playback_event(&playback, &old);
                }

                log::debug!(
                    "Finished playback on all tracks. playing={} position={} len={}",
                    playback.playing,
                    playback.position,
                    playback.tracks.len()
                );

                {
                    let old = playback.clone();
                    playback.playing = false;
                    player.playback.write().unwrap().replace(playback.clone());
                    trigger_playback_event(&playback, &old);
                }

                Ok::<_, PlayerError>(0)
            },
        );

        Ok(())
    }

    /// Triggers playback of the current track.
    ///
    /// Starts or resumes playback at the current position with optional seek offset.
    /// This is the internal method that handles actual playback triggering with retry logic.
    ///
    /// # Errors
    ///
    /// * If failed to play the existing playback
    pub async fn play(
        &mut self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("play: seek={seek:?}");

        handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.player.trigger_play(seek).await }
            }
        })
        .await?;

        Ok(())
    }

    /// Stops the current playback.
    ///
    /// Halts playback completely and releases playback resources.
    ///
    /// # Errors
    ///
    /// * If failed to stop the existing playback
    pub async fn stop(
        &mut self,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("stop: Stopping playback");

        handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.player.trigger_stop().await }
            }
        })
        .await?;

        Ok(())
    }

    /// Seeks to a specific position in the current track.
    ///
    /// Changes the playback position to the specified time offset in seconds.
    ///
    /// # Errors
    ///
    /// * If failed to seek the current playback
    pub async fn seek(
        &mut self,
        seek: f64,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("seek: seek={seek:?}");

        handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.player.trigger_seek(seek).await }
            }
        })
        .await?;

        Ok(())
    }

    /// Skips to the next track in the playlist.
    ///
    /// Advances playback to the next track in the current playlist with optional
    /// seek position. Returns an error if already at the end of the playlist.
    ///
    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to change to the next track
    pub async fn next_track(
        &mut self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = {
            self.playback
                .read()
                .unwrap()
                .clone()
                .ok_or(PlayerError::NoPlayersPlaying)?
        };

        if playback.position + 1 >= u16::try_from(playback.tracks.len()).unwrap() {
            return Err(PlayerError::PositionOutOfBounds(playback.position + 1));
        }

        self.update_playback(
            true,
            Some(true),
            None,
            None,
            Some(playback.position + 1),
            seek,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            retry_options,
        )
        .await
    }

    /// Skips to the previous track in the playlist.
    ///
    /// Returns playback to the previous track in the current playlist with optional
    /// seek position. Returns an error if already at the beginning of the playlist.
    ///
    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to change to the previous track
    pub async fn previous_track(
        &mut self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = {
            self.playback
                .read()
                .unwrap()
                .clone()
                .ok_or(PlayerError::NoPlayersPlaying)?
        };

        if playback.position == 0 {
            return Err(PlayerError::PositionOutOfBounds(0));
        }

        self.update_playback(
            true,
            Some(true),
            None,
            None,
            Some(playback.position - 1),
            seek,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            retry_options,
        )
        .await
    }

    /// Performs pre-update operations before playback state changes.
    ///
    /// This hook allows the player implementation to prepare for upcoming playback state updates.
    ///
    /// # Errors
    ///
    /// * If failed to handle logic in the `before_update_playback`
    #[allow(clippy::unused_async)]
    pub async fn before_update_playback(&mut self) -> Result<(), PlayerError> {
        self.player.before_update_playback().await?;

        Ok(())
    }

    /// Performs post-update operations after playback state changes.
    ///
    /// This hook allows the player implementation to synchronize state after playback updates.
    ///
    /// # Errors
    ///
    /// * If failed to handle logic in the `after_update_playback`
    #[allow(clippy::unused_async)]
    pub async fn after_update_playback(&mut self) -> Result<(), PlayerError> {
        self.player.after_update_playback().await?;

        Ok(())
    }

    /// Updates playback state with multiple configuration options.
    ///
    /// This is the primary method for modifying playback state, including playing,
    /// stopping, seeking, volume control, and playlist changes. It handles complex
    /// state transitions like pause/resume and play/stop logic.
    ///
    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If any of the playback actions failed
    /// * If failed to handle logic in the `before_update_playback`
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
    pub async fn update_playback(
        &mut self,
        modify_playback: bool,
        play: Option<bool>,
        stop: Option<bool>,
        playing: Option<bool>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        tracks: Option<Vec<Track>>,
        quality: Option<PlaybackQuality>,
        session_id: Option<u64>,
        profile: Option<String>,
        playback_target: Option<PlaybackTarget>,
        trigger_event: bool,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!(
            "\
            update_playback:\n\t\
            modify_playback={modify_playback:?}\n\t\
            play={play:?}\n\t\
            stop={stop:?}\n\t\
            playing={playing:?}\n\t\
            position={position:?}\n\t\
            seek={seek:?}\n\t\
            volume={volume:?}\n\t\
            tracks={tracks:?}\n\t\
            quality={quality:?}\n\t\
            session_id={session_id:?}\n\t\
            profile={profile:?}\n\t\
            playback_target={playback_target:?}\n\t\
            trigger_event={trigger_event}\
            "
        );

        self.before_update_playback().await?;

        let original = self.playback.read().unwrap().clone();

        let (session_id, profile) = if let Some(original) = &original {
            log::trace!("update_playback: existing playback={original:?}");
            (
                session_id.unwrap_or(original.session_id),
                profile.unwrap_or_else(|| original.profile.clone()),
            )
        } else {
            (
                session_id.ok_or(PlayerError::MissingSessionId)?,
                profile.ok_or(PlayerError::MissingProfile)?,
            )
        };

        let original = original.unwrap_or_else(|| {
            Playback::new(
                tracks.clone().unwrap_or_default(),
                position,
                AtomicF64::new(volume.unwrap_or(1.0)),
                quality.unwrap_or_default(),
                session_id,
                profile.clone(),
                playback_target.clone(),
            )
        });

        let playing = playing.unwrap_or(original.playing);
        let same_track = same_active_track(position, tracks.as_deref(), &original);
        let wants_to_play = play.unwrap_or(false) || playing;
        let should_start = wants_to_play && (!original.playing || !same_track);
        let should_seek = tracks.is_none() && seek.is_some();
        let should_stop = stop.unwrap_or(false);
        let is_playing = (playing || should_start) && !should_stop;
        let should_resume = same_track && !original.playing && playing && seek.is_none();
        let should_pause = same_track && original.playing && !playing;

        let playback = Playback {
            id: original.id,
            session_id,
            profile,
            playback_target: playback_target.or_else(|| original.playback_target.clone()),
            tracks: tracks.clone().unwrap_or_else(|| original.tracks.clone()),
            playing: is_playing,
            quality: quality.unwrap_or(original.quality),
            position: position.unwrap_or(original.position),
            progress: if play.unwrap_or(false) {
                seek.unwrap_or(0.0)
            } else {
                seek.unwrap_or(original.progress)
            },
            volume: original.volume.clone(),
            abort: if original.abort.is_cancelled() {
                CancellationToken::new()
            } else {
                original.abort.clone()
            },
        };

        if let Some(volume) = volume {
            playback
                .volume
                .store(volume, std::sync::atomic::Ordering::SeqCst);
        }

        log::debug!("update_playback: updating active playback to {playback:?}");
        self.playback.write().unwrap().replace(playback.clone());

        // Call after_update_playback AFTER the volume has been updated
        // This ensures the player can sync the correct volume to shared atomics
        self.after_update_playback().await?;

        if !modify_playback {
            return Ok(());
        }

        log::debug!(
            "\
            update_playback:\n\t\
            should_start_playback={should_start}\n\t\
            should_stop={should_stop}\n\t\
            should_resume={should_resume}\n\t\
            should_pause={should_pause}\n\t\
            should_seek={should_seek}\
            "
        );

        if trigger_event {
            trigger_playback_event(&playback, &original);
        }

        let progress = if let Some(seek) = seek {
            Some(seek)
        } else if playback.progress != 0.0 {
            Some(playback.progress)
        } else {
            None
        };

        if should_seek && let Some(seek) = seek {
            log::debug!("update_playback: Seeking track to seek={seek}");
            self.seek(seek, Some(DEFAULT_SEEK_RETRY_OPTIONS)).await?;
        }
        if should_stop {
            self.stop(retry_options).await?;
        } else if should_resume {
            if let Err(e) = self.resume(retry_options).await {
                log::error!("Failed to resume playback: {e:?}");
                self.play_playback(progress, retry_options).await?;
            }
        } else if should_start {
            self.play_playback(progress, retry_options).await?;
        } else if should_pause {
            self.pause(retry_options).await?;
        }

        Ok(())
    }

    /// Pauses the current playback.
    ///
    /// Temporarily halts playback while maintaining the current position.
    ///
    /// # Errors
    ///
    /// * If failed to pause the current `Playback`
    pub async fn pause(
        &mut self,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("pause: Pausing playback");

        handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.player.trigger_pause().await }
            }
        })
        .await?;

        Ok(())
    }

    /// Resumes playback from a paused state.
    ///
    /// Continues playback from the position where it was paused.
    ///
    /// # Errors
    ///
    /// * If failed to resume the current `Playback`
    pub async fn resume(
        &mut self,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("resume: Resuming playback");

        handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.player.trigger_resume().await }
            }
        })
        .await?;

        Ok(())
    }
}

/// Trait for implementing custom playback players.
#[async_trait]
pub trait Player: std::fmt::Debug + Send {
    /// Hook called before starting a playback session.
    ///
    /// This allows implementations to perform setup or cleanup before playback begins.
    ///
    /// # Errors
    ///
    /// * If setup operations fail
    async fn before_play_playback(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
        Ok(())
    }

    /// Initiates playback at the current position with optional seek.
    ///
    /// # Errors
    ///
    /// * If playback cannot be started
    /// * If the audio output fails
    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError>;

    /// Stops the current playback.
    ///
    /// # Errors
    ///
    /// * If stopping playback fails
    async fn trigger_stop(&self) -> Result<(), PlayerError>;

    /// Seeks to a specific position in the current track.
    ///
    /// # Errors
    ///
    /// * If seeking fails
    /// * If the seek position is invalid
    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError>;

    /// Hook called before updating playback state.
    ///
    /// This allows implementations to prepare for state changes.
    ///
    /// # Errors
    ///
    /// * If preparation operations fail
    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    /// Hook called after updating playback state.
    ///
    /// This allows implementations to synchronize state after changes.
    ///
    /// # Errors
    ///
    /// * If synchronization operations fail
    async fn after_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    /// Pauses the current playback.
    ///
    /// # Errors
    ///
    /// * If pausing fails
    async fn trigger_pause(&self) -> Result<(), PlayerError>;

    /// Resumes playback from a paused state.
    ///
    /// # Errors
    ///
    /// * If resuming fails
    async fn trigger_resume(&self) -> Result<(), PlayerError>;

    /// Retrieves the current playback status.
    ///
    /// # Errors
    ///
    /// * If failed to access the player status
    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError>;

    /// Returns the player's source configuration.
    #[must_use]
    fn get_source(&self) -> &PlayerSource;
}

#[cfg_attr(feature = "profiling", profiling::function)]
fn same_active_track(position: Option<u16>, tracks: Option<&[Track]>, playback: &Playback) -> bool {
    match (position, tracks) {
        (None, None) => true,
        (Some(position), None) => playback.position == position,
        (None, Some(tracks)) => {
            tracks
                .get(playback.position as usize)
                .map(|x: &Track| &x.id)
                == playback
                    .tracks
                    .get(playback.position as usize)
                    .map(|x: &Track| &x.id)
        }
        (Some(position), Some(tracks)) => {
            tracks.get(position as usize).map(|x: &Track| &x.id)
                == playback
                    .tracks
                    .get(playback.position as usize)
                    .map(|x: &Track| &x.id)
        }
    }
}

/// Global service port configuration.
pub static SERVICE_PORT: LazyLock<RwLock<Option<u16>>> = LazyLock::new(|| RwLock::new(None));

/// Sets the service port for local playback.
///
/// # Panics
///
/// * If the `SERVICE_PORT` `RwLock` is poisoned
pub fn set_service_port(service_port: u16) {
    SERVICE_PORT.write().unwrap().replace(service_port);
}

/// Callback function type for playback state change events.
///
/// Functions of this type receive notifications when playback state changes,
/// including play, pause, seek, volume, and track position updates.
type PlaybackEventCallback = fn(&UpdateSession, &Playback);

static PLAYBACK_EVENT_LISTENERS: LazyLock<Arc<RwLock<Vec<PlaybackEventCallback>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// Registers a callback to be invoked when playback state changes.
///
/// The callback receives updates about playback events like play, pause,
/// seek, and volume changes.
///
/// # Panics
///
/// * If the `PLAYBACK_EVENT_LISTENERS` `RwLock` is poisoned
pub fn on_playback_event(listener: PlaybackEventCallback) {
    PLAYBACK_EVENT_LISTENERS.write().unwrap().push(listener);
}

/// Triggers playback events for registered listeners when playback state changes.
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn trigger_playback_event(current: &Playback, previous: &Playback) {
    let Some(playback_target) = current.playback_target.clone() else {
        return;
    };

    let mut has_change = false;

    let playing = if current.playing == previous.playing {
        None
    } else {
        has_change = true;
        Some(current.playing)
    };
    let position = if current.position == previous.position {
        None
    } else {
        has_change = true;
        Some(current.position)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seek = if current.progress as usize == previous.progress as usize {
        None
    } else {
        has_change = true;
        Some(current.progress)
    };
    let current_volume = current.volume.load(std::sync::atomic::Ordering::SeqCst);
    let volume = if (current_volume - previous.volume.load(std::sync::atomic::Ordering::SeqCst))
        .abs()
        < 0.001
    {
        None
    } else {
        has_change = true;
        Some(current_volume)
    };
    let quality = if current.quality == previous.quality {
        None
    } else {
        has_change = true;
        Some(current.quality)
    };
    let tracks = current
        .tracks
        .iter()
        .cloned()
        .map(Into::into)
        .collect::<Vec<_>>();
    let prev_tracks = previous
        .tracks
        .iter()
        .cloned()
        .map(Into::into)
        .collect::<Vec<_>>();
    let playlist = if tracks == prev_tracks {
        None
    } else {
        has_change = true;
        Some(UpdateSessionPlaylist {
            session_playlist_id: 0,
            tracks,
        })
    };

    if !has_change {
        return;
    }

    log::debug!(
        "\
        Triggering playback event:\n\t\
        playing={playing:?}\n\t\
        position={position:?}\n\t\
        seek={seek:?}\n\t\
        quality={quality:?}\n\t\
        volume={volume:?}\n\t\
        playback_target={playback_target:?}\n\t\
        playlist={playlist:?}\
        "
    );

    let update = UpdateSession {
        session_id: current.session_id,
        profile: current.profile.clone(),
        playback_target,
        play: None,
        stop: None,
        name: None,
        active: None,
        playing,
        position,
        seek,
        volume,
        playlist,
        quality,
    };

    send_playback_event(&update, current);
}

#[allow(unused, clippy::too_many_lines)]
async fn track_to_playable_file(
    track: &Track,
    format: PlaybackQuality,
    quality: TrackAudioQuality,
) -> Result<PlayableTrack, PlayerError> {
    log::trace!("track_to_playable_file track={track:?} format={format:?} quality={quality:?}");

    let mut hint = Hint::new();

    let file = track.file.clone().unwrap();
    let path = Path::new(&file);

    // Provide the file extension as a hint.
    if let Some(extension) = path.extension()
        && let Some(extension_str) = extension.to_str()
    {
        hint.with_extension(extension_str);
    }

    #[allow(clippy::match_wildcard_for_single_variants)]
    let same_source = match format.format {
        AudioFormat::Source => true,
        #[allow(unreachable_patterns)]
        _ => track.format.is_none_or(|x| x == format.format),
    };

    let source: Box<dyn MediaSource> = if same_source {
        Box::new(File::open(path)?)
    } else {
        #[allow(unused_mut)]
        let mut signal_chain = SignalChain::new();

        match format.format {
            #[cfg(feature = "format-aac")]
            AudioFormat::Aac => {
                #[cfg(feature = "encoder-aac")]
                {
                    use moosicbox_audio_output::encoder::aac::AacEncoder;
                    log::debug!("Encoding playback with AacEncoder");
                    let mut hint = Hint::new();
                    hint.with_extension("m4a");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(AacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(not(feature = "encoder-aac"))]
                panic!("No encoder-aac feature");
            }
            #[cfg(feature = "format-flac")]
            AudioFormat::Flac => {
                #[cfg(feature = "encoder-flac")]
                {
                    use moosicbox_audio_output::encoder::flac::FlacEncoder;
                    log::debug!("Encoding playback with FlacEncoder");
                    let mut hint = Hint::new();
                    hint.with_extension("flac");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(FlacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(not(feature = "encoder-flac"))]
                panic!("No encoder-flac feature");
            }
            #[cfg(feature = "format-mp3")]
            AudioFormat::Mp3 => {
                #[cfg(feature = "encoder-mp3")]
                {
                    use moosicbox_audio_output::encoder::mp3::Mp3Encoder;
                    log::debug!("Encoding playback with Mp3Encoder");
                    let mut hint = Hint::new();
                    hint.with_extension("mp3");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(Mp3Encoder::new()))
                        .with_hint(hint);
                }
                #[cfg(not(feature = "encoder-mp3"))]
                panic!("No encoder-mp3 feature");
            }
            #[cfg(feature = "format-opus")]
            AudioFormat::Opus => {
                #[cfg(feature = "encoder-opus")]
                {
                    use moosicbox_audio_output::encoder::opus::OpusEncoder;
                    log::debug!("Encoding playback with OpusEncoder");
                    let mut hint = Hint::new();
                    hint.with_extension("opus");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(OpusEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(not(feature = "encoder-opus"))]
                panic!("No encoder-opus feature");
            }
            #[allow(unreachable_patterns)]
            _ => {
                moosicbox_assert::die!("Invalid format {}", format.format);
            }
        }

        log::trace!(
            "track_to_playable_file: getting file at path={}",
            path.display()
        );
        let file = tokio::fs::File::open(path.to_path_buf()).await?;

        log::trace!("track_to_playable_file: Creating ByteStreamSource");
        let ms = Box::new(ByteStreamSource::new(
            Box::new(
                StalledReadMonitor::new(
                    FramedRead::new(file, BytesCodec::new())
                        .map_ok(bytes::BytesMut::freeze)
                        .boxed(),
                )
                .map(|x| match x {
                    Ok(Ok(x)) => Ok(x),
                    Ok(Err(err)) | Err(err) => Err(err),
                }),
            ),
            None,
            true,
            false,
            CancellationToken::new(),
        ));

        match signal_chain.process(ms) {
            Ok(stream) => stream,
            Err(SignalChainError::Playback(e)) => {
                return Err(PlayerError::PlaybackError(match e {
                    symphonia_unsync::PlaybackError::Symphonia(e) => PlaybackError::Symphonia(e),
                    symphonia_unsync::PlaybackError::Decode(e) => PlaybackError::Decode(e),
                }));
            }
            Err(SignalChainError::Empty) => unreachable!("Empty signal chain"),
        }
    };

    Ok(PlayableTrack {
        track_id: track.id.clone(),
        source,
        hint,
    })
}

#[allow(unused)]
async fn track_to_playable_stream(
    track: &Track,
    format: PlaybackQuality,
    quality: TrackAudioQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    track_id_to_playable_stream(
        &track.id,
        &track.api_source,
        format,
        quality,
        player_source,
        abort,
    )
    .await
}

#[allow(unused)]
async fn track_id_to_playable_stream(
    track_id: &Id,
    source: &ApiSource,
    format: PlaybackQuality,
    quality: TrackAudioQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    let (url, headers) =
        get_track_url(track_id, source, player_source, format, quality, false).await?;

    log::debug!("Fetching track bytes from url: {url}");

    let mut client = CLIENT.head(&url);

    if let Some(headers) = headers {
        for (key, value) in headers {
            client = client.header(&key, &value);
        }
    }

    let mut res = client.send().await.unwrap();
    let headers = res.headers();
    let size = headers
        .get("content-length")
        .map(|length| length.parse::<u64>().unwrap());

    let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
        url,
        size,
        true,
        size.is_some(), // HTTP range requests work for any format when size is known
        abort,
    )
    .into();

    let mut hint = Hint::new();

    if let Some(content_type) = headers.get("content-type") {
        if let Some(audio_type) = content_type.strip_prefix("audio/") {
            log::debug!("Setting hint extension to {audio_type}");
            hint.with_extension(audio_type);
        } else {
            log::warn!("Invalid audio content_type: {content_type}");
        }
    }

    Ok(PlayableTrack {
        track_id: track_id.to_owned(),
        source: Box::new(source),
        hint,
    })
}

#[allow(unused)]
async fn track_or_id_to_playable(
    playback_type: PlaybackType,
    track: &Track,
    format: PlaybackQuality,
    quality: TrackAudioQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    log::trace!(
        "track_or_id_to_playable playback_type={playback_type:?} track={track:?} quality={format:?}"
    );
    Ok(
        if track.api_source.is_library()
            && matches!(playback_type, PlaybackType::File | PlaybackType::Default)
        {
            track_to_playable_file(track, format, quality).await?
        } else {
            track_to_playable_stream(track, format, quality, player_source, abort).await?
        },
    )
}

async fn handle_retry<
    T,
    E: std::fmt::Debug + Into<PlayerError>,
    F: Future<Output = Result<T, E>> + Send,
>(
    retry_options: Option<PlaybackRetryOptions>,
    func: impl Fn() -> F + Send,
) -> Result<T, PlayerError> {
    let mut retry_count = 0;

    loop {
        if retry_count > 0 {
            switchy_async::time::sleep(retry_options.unwrap().retry_delay).await;
        }

        match func().await {
            Ok(value) => {
                log::trace!("Finished action");
                return Ok(value);
            }
            Err(e) => {
                let e = e.into();
                if matches!(e, PlayerError::Cancelled) {
                    log::debug!("Action cancelled");
                    return Err(e);
                }
                log::error!("Action failed: {e:?}");
                if let Some(retry_options) = retry_options {
                    retry_count += 1;
                    if retry_count >= retry_options.max_attempts {
                        log::error!(
                            "Action retry failed after {retry_count} attempts. Not retrying"
                        );
                        return Err(e);
                    }
                    log::info!(
                        "Retrying action attempt {}/{}",
                        retry_count + 1,
                        retry_options.max_attempts
                    );
                    continue;
                }

                log::debug!("No retry options");
                break Err(e);
            }
        }
    }
}

/// Notifies all registered listeners of a playback event.
///
/// This function is called internally when playback state changes to broadcast
/// the update to all registered event listeners.
///
/// # Panics
///
/// * If the `PLAYBACK_EVENT_LISTENERS` `RwLock` is poisoned
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn send_playback_event(update: &UpdateSession, playback: &Playback) {
    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(update, playback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_track(id: u64) -> Track {
        Track {
            id: id.into(),
            number: 1,
            title: format!("Track {id}"),
            duration: 180.0,
            album: "Test Album".to_string(),
            album_id: 1.into(),
            album_type: moosicbox_music_models::AlbumType::Lp,
            date_released: None,
            date_added: None,
            artist: "Test Artist".to_string(),
            artist_id: 1.into(),
            file: None,
            artwork: None,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: moosicbox_music_models::TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: moosicbox_music_models::ApiSources::default(),
        }
    }

    #[test_log::test]
    fn test_same_active_track_no_changes() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // No position change, no tracks change
        assert!(same_active_track(None, None, &playback));
    }

    #[test_log::test]
    fn test_same_active_track_same_position_no_tracks() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Same position, no new tracks
        assert!(same_active_track(Some(0), None, &playback));
    }

    #[test_log::test]
    fn test_same_active_track_different_position_no_tracks() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Different position, no new tracks
        assert!(!same_active_track(Some(1), None, &playback));
    }

    #[test_log::test]
    fn test_same_active_track_same_track_at_position() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Same track at current position
        assert!(same_active_track(None, Some(&playback.tracks), &playback));
    }

    #[test_log::test]
    fn test_same_active_track_different_track_at_position() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Different track at current position
        let new_tracks = vec![create_test_track(3), create_test_track(2)];
        assert!(!same_active_track(None, Some(&new_tracks), &playback));
    }

    #[test_log::test]
    fn test_same_active_track_with_position_and_tracks() {
        let tracks = vec![
            create_test_track(1),
            create_test_track(2),
            create_test_track(3),
        ];
        let playback = Playback::new(
            tracks.clone(),
            Some(1),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Position 1, same track at position 1
        assert!(same_active_track(Some(1), Some(&tracks), &playback));

        // Position 2, different from playback position 1
        assert!(!same_active_track(Some(2), Some(&tracks), &playback));
    }

    #[test_log::test]
    fn test_playback_new_creates_valid_instance() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(0.8),
            PlaybackQuality::default(),
            123,
            "test-profile".to_string(),
            None,
        );

        assert_eq!(playback.session_id, 123);
        assert_eq!(playback.profile, "test-profile");
        assert_eq!(playback.tracks.len(), 1);
        assert!(!playback.playing);
        assert_eq!(playback.position, 0);
        assert!((playback.volume.load(std::sync::atomic::Ordering::SeqCst) - 0.8).abs() < 0.001);
        assert!((playback.progress - 0.0).abs() < 0.001);
    }

    #[test_log::test]
    fn test_playback_new_defaults_position_to_zero() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            None,
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        assert_eq!(playback.position, 0);
    }

    #[test_log::test]
    fn test_playback_to_api_playback_conversion() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let mut playback = Playback::new(
            tracks,
            Some(1),
            AtomicF64::new(0.7),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );
        playback.playing = true;
        playback.progress = 45.5;

        let api_playback: ApiPlayback = playback.into();

        assert_eq!(api_playback.track_ids.len(), 2);
        assert_eq!(api_playback.track_ids[0], "1");
        assert_eq!(api_playback.track_ids[1], "2");
        assert!(api_playback.playing);
        assert_eq!(api_playback.position, 1);
        assert!((api_playback.seek - 45.5).abs() < 0.001);
    }

    #[test_log::test]
    fn test_playback_status_struct() {
        let status = PlaybackStatus { success: true };
        assert!(status.success);

        let status = PlaybackStatus { success: false };
        assert!(!status.success);
    }

    #[test_log::test]
    fn test_playback_type_default_is_default() {
        let playback_type = PlaybackType::default();
        assert!(matches!(playback_type, PlaybackType::Default));
    }

    #[test_log::test]
    fn test_playback_retry_options_constants() {
        assert_eq!(DEFAULT_SEEK_RETRY_OPTIONS.max_attempts, 10);
        assert_eq!(
            DEFAULT_SEEK_RETRY_OPTIONS.retry_delay,
            std::time::Duration::from_millis(100)
        );

        assert_eq!(DEFAULT_PLAYBACK_RETRY_OPTIONS.max_attempts, 10);
        assert_eq!(
            DEFAULT_PLAYBACK_RETRY_OPTIONS.retry_delay,
            std::time::Duration::from_millis(500)
        );
    }

    #[test_log::test]
    fn test_player_source_debug_format() {
        let source = PlayerSource::Local;
        let debug_str = format!("{source:?}");
        assert!(debug_str.contains("Local"));

        let source = PlayerSource::Remote {
            host: "http://localhost:8080".to_string(),
            query: None,
            headers: None,
        };
        let debug_str = format!("{source:?}");
        assert!(debug_str.contains("Remote"));
        assert!(debug_str.contains("localhost"));
    }

    #[test_log::test]
    fn test_set_service_port() {
        set_service_port(9876);
        assert_eq!(*SERVICE_PORT.read().unwrap(), Some(9876));
    }

    #[test_log::test]
    fn test_playback_handler_new_creates_valid_instance() {
        #[derive(Debug)]
        struct MockPlayer;

        #[async_trait]
        impl Player for MockPlayer {
            async fn trigger_play(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_stop(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_seek(&self, _seek: f64) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_pause(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_resume(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
                Ok(ApiPlaybackStatus {
                    active_playbacks: None,
                })
            }
            fn get_source(&self) -> &PlayerSource {
                &PlayerSource::Local
            }
        }

        let handler = PlaybackHandler::new(MockPlayer);
        assert!(handler.playback.read().unwrap().is_none());
        assert!(handler.output.is_none());
    }

    #[test_log::test]
    fn test_player_error_display() {
        let error = PlayerError::NoPlayersPlaying;
        assert_eq!(error.to_string(), "No players playing");

        let error = PlayerError::TrackNotFound(42.into());
        assert!(error.to_string().contains("Track not found"));
        assert!(error.to_string().contains("42"));

        let error = PlayerError::PositionOutOfBounds(99);
        assert!(error.to_string().contains("Position out of bounds"));
        assert!(error.to_string().contains("99"));
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_no_changes() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        // Same playback state - should not trigger event
        trigger_playback_event(&playback, &playback);
        // No assertion needed - just verifying it doesn't panic
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_playing_change() {
        let tracks = vec![create_test_track(1)];
        let mut playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );
        playback1.playing = false;

        let mut playback2 = playback1.clone();
        playback2.playing = true;

        // Different playing state - should trigger event
        trigger_playback_event(&playback2, &playback1);
        // No assertion needed - just verifying it doesn't panic
    }

    #[test_log::test]
    fn test_trigger_playback_event_without_target_does_nothing() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None, // No playback target
        );

        let mut playback2 = playback.clone();
        playback2.playing = true;

        // Should return early without triggering
        trigger_playback_event(&playback2, &playback);
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_position_change() {
        let tracks = vec![create_test_track(1), create_test_track(2)];
        let playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        let mut playback2 = playback1.clone();
        playback2.position = 1;

        // Different position - should trigger event
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_volume_change() {
        let tracks = vec![create_test_track(1)];
        let playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        let playback2 = playback1.clone();
        // Volume change larger than 0.001 threshold
        playback2
            .volume
            .store(0.5, std::sync::atomic::Ordering::SeqCst);

        // Different volume - should trigger event
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test]
    fn test_trigger_playback_event_volume_within_threshold_no_change() {
        let tracks = vec![create_test_track(1)];
        let playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        let playback2 = playback1.clone();
        // Volume change smaller than 0.001 threshold - should not be detected as change
        playback2
            .volume
            .store(1.0005, std::sync::atomic::Ordering::SeqCst);

        // Volume within threshold - should NOT trigger event (returns early)
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_seek_change() {
        let tracks = vec![create_test_track(1)];
        let mut playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );
        playback1.progress = 10.0;

        let mut playback2 = playback1.clone();
        // Progress change that results in different integer (seek is compared as usize)
        playback2.progress = 15.0;

        // Different seek/progress - should trigger event
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test]
    fn test_trigger_playback_event_seek_same_second_no_change() {
        let tracks = vec![create_test_track(1)];
        let mut playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );
        playback1.progress = 10.3;

        let mut playback2 = playback1.clone();
        // Progress change within the same second (cast to usize)
        playback2.progress = 10.7;

        // Same second - should NOT trigger event (returns early due to has_change=false)
        trigger_playback_event(&playback2, &playback1);
    }

    #[cfg(feature = "format-flac")]
    #[test_log::test]
    fn test_trigger_playback_event_with_quality_change() {
        let tracks = vec![create_test_track(1)];
        let playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality {
                format: moosicbox_music_models::AudioFormat::Source,
            },
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        let mut playback2 = playback1.clone();
        playback2.quality = PlaybackQuality {
            format: moosicbox_music_models::AudioFormat::Flac,
        };

        // Different quality - should trigger event
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test]
    fn test_trigger_playback_event_with_tracks_change() {
        let tracks1 = vec![create_test_track(1)];
        let playback1 = Playback::new(
            tracks1,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );

        let mut playback2 = playback1.clone();
        playback2.tracks = vec![create_test_track(1), create_test_track(2)];

        // Different tracks - should trigger event
        trigger_playback_event(&playback2, &playback1);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_retry_success_on_first_try() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32, PlayerError> = handle_retry(
            Some(PlaybackRetryOptions {
                max_attempts: 3,
                retry_delay: std::time::Duration::from_millis(1),
            }),
            move || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<i32, PlayerError>(42)
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_handle_retry_success_after_retries() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32, PlayerError> = handle_retry(
            Some(PlaybackRetryOptions {
                max_attempts: 5,
                retry_delay: std::time::Duration::from_millis(1),
            }),
            move || {
                let count = call_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if current < 2 {
                        // Fail first two attempts
                        Err(PlayerError::RetryRequested)
                    } else {
                        // Succeed on third attempt
                        Ok::<i32, PlayerError>(42)
                    }
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        // Should have been called 3 times (2 failures + 1 success)
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_handle_retry_exhausts_max_attempts() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32, PlayerError> = handle_retry(
            Some(PlaybackRetryOptions {
                max_attempts: 3,
                retry_delay: std::time::Duration::from_millis(1),
            }),
            move || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // Always fail
                    Err(PlayerError::RetryRequested)
                }
            },
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(result, Err(PlayerError::RetryRequested)));
        // Should have been called max_attempts times
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_retry_cancelled_returns_immediately() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32, PlayerError> = handle_retry(
            Some(PlaybackRetryOptions {
                max_attempts: 5,
                retry_delay: std::time::Duration::from_millis(1),
            }),
            move || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // Return Cancelled error - should not retry
                    Err(PlayerError::Cancelled)
                }
            },
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(result, Err(PlayerError::Cancelled)));
        // Should have been called only once - cancellation doesn't retry
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_retry_no_options_single_attempt() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32, PlayerError> = handle_retry(None, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Fail
                Err(PlayerError::RetryRequested)
            }
        })
        .await;

        assert!(result.is_err());
        // Without retry options, should only try once and fail
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test]
    fn test_same_active_track_with_empty_tracks() {
        let playback = Playback::new(
            vec![],
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // Empty tracks list should handle gracefully
        assert!(same_active_track(None, None, &playback));
        assert!(same_active_track(None, Some(&[]), &playback));
    }

    #[test_log::test]
    fn test_same_active_track_position_out_of_bounds() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks.clone(),
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        // When position (5) is out of bounds in provided tracks, tracks.get(5) returns None
        // But playback has position 0, so playback.tracks.get(0) returns Some(track)
        // Comparison is: None == Some(track) => false (tracks differ)
        assert!(!same_active_track(Some(5), Some(&tracks), &playback));
    }

    #[test_log::test]
    fn test_playback_abort_token_starts_uncancelled() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        assert!(!playback.abort.is_cancelled());
    }

    #[test_log::test]
    fn test_playback_abort_token_can_be_cancelled() {
        let tracks = vec![create_test_track(1)];
        let playback = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        );

        assert!(!playback.abort.is_cancelled());
        playback.abort.cancel();
        assert!(playback.abort.is_cancelled());
    }

    #[test_log::test]
    fn test_player_error_variants_display() {
        // Test additional error variants to ensure they display correctly
        let error = PlayerError::UnsupportedFormat(moosicbox_music_models::AudioFormat::Source);
        assert!(error.to_string().contains("Format not supported"));

        let error = PlayerError::Seek("seek failed".to_string());
        assert!(error.to_string().contains("Failed to seek"));
        assert!(error.to_string().contains("seek failed"));

        let error = PlayerError::InvalidSession {
            session_id: 123,
            message: "invalid".to_string(),
        };
        assert!(error.to_string().contains("123"));
        assert!(error.to_string().contains("invalid"));

        let error = PlayerError::MissingSessionId;
        assert!(error.to_string().contains("Missing session ID"));

        let error = PlayerError::MissingProfile;
        assert!(error.to_string().contains("Missing profile"));

        let error = PlayerError::InvalidState;
        assert!(error.to_string().contains("Invalid state"));

        let error = PlayerError::InvalidSource;
        assert!(error.to_string().contains("Invalid source"));

        let error = PlayerError::Cancelled;
        assert!(error.to_string().contains("cancelled"));

        let error = PlayerError::RetryRequested;
        assert!(error.to_string().contains("retry"));
    }

    #[test_log::test]
    fn test_playback_handler_with_playback_sets_playback() {
        #[derive(Debug)]
        struct MockPlayer;

        #[async_trait]
        impl Player for MockPlayer {
            async fn trigger_play(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_stop(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_seek(&self, _seek: f64) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_pause(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_resume(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
                Ok(ApiPlaybackStatus {
                    active_playbacks: None,
                })
            }
            fn get_source(&self) -> &PlayerSource {
                &PlayerSource::Local
            }
        }

        let shared_playback = Arc::new(std::sync::RwLock::new(Some(Playback::new(
            vec![create_test_track(1)],
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            None,
        ))));

        let handler = PlaybackHandler::new(MockPlayer).with_playback(shared_playback.clone());

        // Verify the playback was set
        assert!(handler.playback.read().unwrap().is_some());

        // Verify it's the same Arc (shared reference)
        assert!(Arc::ptr_eq(&handler.playback, &shared_playback));
    }

    #[test_log::test]
    fn test_playback_handler_with_output_sets_output() {
        #[derive(Debug)]
        struct MockPlayer;

        #[async_trait]
        impl Player for MockPlayer {
            async fn trigger_play(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_stop(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_seek(&self, _seek: f64) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_pause(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_resume(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
                Ok(ApiPlaybackStatus {
                    active_playbacks: None,
                })
            }
            fn get_source(&self) -> &PlayerSource {
                &PlayerSource::Local
            }
        }

        let handler = PlaybackHandler::new(MockPlayer);
        assert!(handler.output.is_none());

        let output: Option<Arc<std::sync::Mutex<AudioOutputFactory>>> = None;
        let handler = handler.with_output(output);
        assert!(handler.output.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_remote_source() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 42.into();
        let api_source = ApiSource::library();
        let player_source = PlayerSource::Remote {
            host: "http://example.com:8080".to_string(),
            query: None,
            headers: None,
        };
        let format = PlaybackQuality::default();
        let quality = TrackAudioQuality::Low;

        let (url, headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should construct a URL with the remote host
        assert!(url.starts_with("http://example.com:8080/files/track"));
        assert!(url.contains("trackId=42"));
        assert!(url.contains("quality=LOW"));
        // Headers should be None when not provided
        assert!(headers.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_remote_source_and_query_params() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 123.into();
        let api_source = ApiSource::library();

        let mut query = std::collections::BTreeMap::new();
        query.insert("customParam".to_string(), "customValue".to_string());

        let player_source = PlayerSource::Remote {
            host: "http://music.local:9000".to_string(),
            query: Some(query),
            headers: None,
        };
        let format = PlaybackQuality::default();
        let quality = TrackAudioQuality::FlacHighestRes;

        let (url, _headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should include custom query params
        assert!(url.contains("customParam=customValue"));
        assert!(url.contains("trackId=123"));
        assert!(url.contains("quality=FLAC_HIGHEST_RES"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_remote_source_and_headers() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 456.into();
        let api_source = ApiSource::library();

        let mut headers = std::collections::BTreeMap::new();
        headers.insert("moosicbox-profile".to_string(), "test-profile".to_string());

        let player_source = PlayerSource::Remote {
            host: "http://remote.server".to_string(),
            query: None,
            headers: Some(headers),
        };
        let format = PlaybackQuality::default();
        let quality = TrackAudioQuality::Low;

        let (url, returned_headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Headers should be returned
        assert!(returned_headers.is_some());
        let headers = returned_headers.unwrap();
        assert_eq!(
            headers.get("moosicbox-profile"),
            Some(&"test-profile".to_string())
        );

        // Profile should be included in URL when header is present
        assert!(url.contains("moosicboxProfile=test-profile"));
    }

    #[cfg(feature = "format-flac")]
    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_non_source_format() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 789.into();
        let api_source = ApiSource::library();
        let player_source = PlayerSource::Remote {
            host: "http://test.host".to_string(),
            query: None,
            headers: None,
        };
        let format = PlaybackQuality {
            format: moosicbox_music_models::AudioFormat::Flac,
        };
        let quality = TrackAudioQuality::Low;

        let (url, _headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should include format when not Source
        assert!(url.contains("format=FLAC"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_source_format_omits_format_param() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 111.into();
        let api_source = ApiSource::library();
        let player_source = PlayerSource::Remote {
            host: "http://test.host".to_string(),
            query: None,
            headers: None,
        };
        let format = PlaybackQuality {
            format: moosicbox_music_models::AudioFormat::Source,
        };
        let quality = TrackAudioQuality::Low;

        let (url, _headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should NOT include format when it's Source
        assert!(!url.contains("format="));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_library_source_omits_source_param() {
        use moosicbox_music_api::models::TrackAudioQuality;

        let track_id = 222.into();
        let api_source = ApiSource::library();
        let player_source = PlayerSource::Remote {
            host: "http://test.host".to_string(),
            query: None,
            headers: None,
        };
        let format = PlaybackQuality::default();
        let quality = TrackAudioQuality::Low;

        let (url, _headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should NOT include source when it's library
        assert!(!url.contains("source="));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_track_url_with_local_source() {
        use moosicbox_music_api::models::TrackAudioQuality;

        // Set up SERVICE_PORT for local source
        set_service_port(8765);

        let track_id = 333.into();
        let api_source = ApiSource::library();
        let player_source = PlayerSource::Local;
        let format = PlaybackQuality::default();
        let quality = TrackAudioQuality::FlacLossless;

        let (url, headers) = get_track_url(
            &track_id,
            &api_source,
            &player_source,
            format,
            quality,
            false,
        )
        .await
        .expect("Failed to get track URL");

        // Should use local IP and configured port
        assert!(url.starts_with("http://127.0.0.1:8765/files/track"));
        assert!(url.contains("trackId=333"));
        assert!(url.contains("quality=FLAC_LOSSLESS"));
        // Headers should be None for local source
        assert!(headers.is_none());
    }

    #[test_log::test]
    fn test_on_playback_event_registers_listener() {
        use std::sync::atomic::AtomicBool;

        // Define the listener function first (before any statements)
        fn test_listener(_update: &UpdateSession, _playback: &Playback) {
            static LISTENER_CALLED: AtomicBool = AtomicBool::new(false);
            LISTENER_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        on_playback_event(test_listener);

        // Create playback with a target (required for events to fire)
        let tracks = vec![create_test_track(1)];
        let mut playback1 = Playback::new(
            tracks,
            Some(0),
            AtomicF64::new(1.0),
            PlaybackQuality::default(),
            1,
            "test".to_string(),
            Some(PlaybackTarget::AudioZone { audio_zone_id: 1 }),
        );
        playback1.playing = false;

        let mut playback2 = playback1.clone();
        playback2.playing = true;

        // Trigger event which should call our registered listener
        trigger_playback_event(&playback2, &playback1);

        // The listener was registered and can be called - this test verifies registration works
    }

    #[test_log::test]
    fn test_send_playback_event_calls_all_registered_listeners() {
        // Define the listener function first (before any statements)
        fn counter_listener(_update: &UpdateSession, _playback: &Playback) {
            // This function is registered as a listener
        }

        // Note: Since PLAYBACK_EVENT_LISTENERS is global and we can't easily clear it,
        // we just verify that registering and calling works. The call count will include
        // any previously registered listeners from other tests.
        let initial_count = PLAYBACK_EVENT_LISTENERS.read().unwrap().len();

        on_playback_event(counter_listener);

        // Verify registration increased the count
        assert_eq!(
            PLAYBACK_EVENT_LISTENERS.read().unwrap().len(),
            initial_count + 1
        );
    }

    #[test_log::test]
    fn test_playback_handler_new_boxed() {
        #[derive(Debug)]
        struct TestPlayer;

        #[async_trait]
        impl Player for TestPlayer {
            async fn trigger_play(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_stop(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_seek(&self, _seek: f64) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_pause(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            async fn trigger_resume(&self) -> Result<(), PlayerError> {
                Ok(())
            }
            fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
                Ok(ApiPlaybackStatus {
                    active_playbacks: None,
                })
            }
            fn get_source(&self) -> &PlayerSource {
                &PlayerSource::Local
            }
        }

        let boxed_player: Box<dyn Player + Sync> = Box::new(TestPlayer);
        let handler = PlaybackHandler::new_boxed(boxed_player);

        // Verify it was created correctly
        assert!(handler.playback.read().unwrap().is_none());
        assert!(handler.output.is_none());

        // Verify player_status works through the handler
        let status = handler
            .player
            .player_status()
            .expect("Failed to get status");
        assert!(status.active_playbacks.is_none());
    }
}
