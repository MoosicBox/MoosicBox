#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, LazyLock, RwLock},
};

use ::symphonia::core::{io::MediaSource, probe::Hint};
use async_trait::async_trait;
use atomic_float::AtomicF64;
use flume::{Receiver, SendError, bounded};
use futures::{Future, StreamExt as _, TryStreamExt as _};
use local_ip_address::local_ip;
use moosicbox_audio_decoder::media_sources::{
    bytestream_source::ByteStreamSource, remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_audio_output::AudioOutputFactory;
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_json_utils::{ParseError, database::DatabaseFetchError};
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::{ApiSource, AudioFormat, PlaybackQuality, Track, id::Id};
use moosicbox_session::{
    get_session_playlist,
    models::{ApiSession, PlaybackTarget, Session, UpdateSession, UpdateSessionPlaylist},
};
use moosicbox_stream_utils::{
    remote_bytestream::RemoteByteStream, stalled_monitor::StalledReadMonitor,
};
use rand::{Rng as _, rng};
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::{
    codec::{BytesCodec, FramedRead},
    sync::CancellationToken,
};

use crate::{
    signal_chain::{SignalChain, SignalChainError},
    symphonia::PlaybackError,
};

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "local")]
pub mod local;

pub mod signal_chain;
pub mod symphonia;
pub mod symphonia_unsync;
pub mod volume_mixer;

pub const DEFAULT_SEEK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: std::time::Duration::from_millis(100),
};

pub const DEFAULT_PLAYBACK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: std::time::Duration::from_millis(500),
};

pub static CLIENT: LazyLock<moosicbox_http::Client> = LazyLock::new(moosicbox_http::Client::new);

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(#[from] SendError<()>),
    #[error(transparent)]
    Http(#[from] moosicbox_http::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Acquire(#[from] tokio::sync::AcquireError),
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
}

impl std::fmt::Debug for PlayableTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayableTrack")
            .field("track_id", &self.track_id)
            .field("source", &"{{source}}")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct Playback {
    pub id: u64,
    pub session_id: u64,
    pub profile: String,
    pub tracks: Vec<Track>,
    pub playing: bool,
    pub position: u16,
    pub quality: PlaybackQuality,
    pub progress: f64,
    pub volume: Arc<AtomicF64>,
    pub playback_target: Option<PlaybackTarget>,
    pub abort: CancellationToken,
}

impl Playback {
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
            id: rng().random::<u64>(),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlayback {
    pub track_ids: Vec<String>,
    pub playing: bool,
    pub position: u16,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlaybackStatus {
    pub active_playbacks: Option<ApiPlayback>,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlaybackStatus {
    pub success: bool,
}

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
    api_source: ApiSource,
    player_source: &PlayerSource,
    quality: PlaybackQuality,
    use_local_network_ip: bool,
) -> Result<(String, Option<HashMap<String, String>>), PlayerError> {
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
                host.to_string()
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

        serializer.append_pair("trackId", &track_id.to_string());
        serializer.append_pair("source", api_source.as_ref());

        if let Some(profile) = headers
            .as_ref()
            .and_then(|x| x.get("moosicbox-profile").cloned())
        {
            serializer.append_pair("moosicboxProfile", &profile);
        }

        match api_source {
            ApiSource::Library => {
                if quality.format != AudioFormat::Source {
                    serializer.append_pair("format", quality.format.as_ref());
                }
            }
            #[cfg(feature = "tidal")]
            ApiSource::Tidal => {
                serializer.append_pair("audioQuality", "HIGH");
            }
            #[cfg(feature = "qobuz")]
            ApiSource::Qobuz => {
                serializer.append_pair("audioQuality", "LOW");
            }
            #[cfg(feature = "yt")]
            ApiSource::Yt => {
                serializer.append_pair("audioQuality", "LOW");
            }
        }

        serializer.finish()
    };

    let query_string = format!("?{query_params}");

    let url = match api_source {
        ApiSource::Library => Ok::<_, PlayerError>(format!("{host}/files/track{query_string}")),
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => {
            use moosicbox_json_utils::serde_json::ToValue as _;
            let url = format!("{host}/tidal/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            CLIENT
                .get(&url)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?
                .to_value::<Vec<String>>("urls")?
                .first()
                .cloned()
                .ok_or(PlayerError::TrackFetchFailed(track_id.to_string()))
        }
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => {
            use moosicbox_json_utils::serde_json::ToValue as _;
            let url = format!("{host}/qobuz/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            Ok(CLIENT
                .get(&url)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?
                .to_value::<String>("url")?)
        }
        #[cfg(feature = "yt")]
        ApiSource::Yt => {
            use moosicbox_json_utils::serde_json::ToValue as _;
            let url = format!("{host}/yt/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            Ok(CLIENT
                .get(&url)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?
                .to_value::<String>("url")?)
        }
    }?;

    Ok((url, headers))
}

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

pub struct PlayableTrack {
    pub track_id: Id,
    pub source: Box<dyn MediaSource>,
    pub hint: Hint,
}

#[derive(Copy, Clone, Default, Deserialize, Serialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaybackType {
    File,
    Stream,
    #[default]
    Default,
}

#[derive(Copy, Clone)]
pub struct PlaybackRetryOptions {
    pub max_attempts: u32,
    pub retry_delay: std::time::Duration,
}

#[derive(Debug, Clone)]
pub enum PlayerSource {
    Local,
    Remote {
        host: String,
        query: Option<HashMap<String, String>>,
        headers: Option<HashMap<String, String>>,
    },
}

#[derive(Debug, Clone)]
pub struct PlaybackHandler {
    pub id: u64,
    pub playback: Arc<std::sync::RwLock<Option<Playback>>>,
    pub output: Option<Arc<std::sync::Mutex<AudioOutputFactory>>>,
    pub player: Arc<Box<dyn Player + Sync>>,
    receiver: Arc<tokio::sync::RwLock<Option<Receiver<()>>>>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl PlaybackHandler {
    pub fn new(player: impl Player + Sync + 'static) -> Self {
        Self::new_boxed(Box::new(player))
    }

    #[must_use]
    pub fn new_boxed(player: Box<dyn Player + Sync>) -> Self {
        let playback = Arc::new(std::sync::RwLock::new(None));
        let output = None;
        let receiver = Arc::new(tokio::sync::RwLock::new(None));

        Self {
            id: rng().random::<u64>(),
            playback,
            output,
            player: Arc::new(player),
            receiver,
        }
    }

    #[must_use]
    pub fn with_playback(mut self, playback: Arc<std::sync::RwLock<Option<Playback>>>) -> Self {
        self.playback = playback;
        self
    }

    #[must_use]
    pub fn with_output(
        mut self,
        output: Option<Arc<std::sync::Mutex<AudioOutputFactory>>>,
    ) -> Self {
        self.output = output;
        self
    }

    #[must_use]
    pub fn with_receiver(
        mut self,
        receiver: Arc<tokio::sync::RwLock<Option<Receiver<()>>>>,
    ) -> Self {
        self.receiver = receiver;
        self
    }
}

impl PlaybackHandler {
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

        let (tx, rx) = bounded(1);
        self.receiver.write().await.replace(rx);

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

        moosicbox_task::spawn("player: Play playback", async move {
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
                tokio::select! {
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

                            tx.send_async(()).await?;
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

            tx.send_async(()).await?;

            Ok::<_, PlayerError>(0)
        });

        Ok(())
    }

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

    /// # Errors
    ///
    /// * If failed to handle logic in the `before_update_playback`
    #[allow(clippy::unused_async)]
    pub async fn before_update_playback(&mut self) -> Result<(), PlayerError> {
        Ok(())
    }

    /// # Panics
    ///
    /// * If the `playback` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If any of the playback actions failed
    /// * If failed to handle logic in the `before_update_playback`
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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

        if should_seek {
            if let Some(seek) = seek {
                log::debug!("update_playback: Seeking track to seek={seek}");
                self.seek(seek, Some(DEFAULT_SEEK_RETRY_OPTIONS)).await?;
            }
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

#[async_trait]
pub trait Player: std::fmt::Debug + Send {
    async fn before_play_playback(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
        Ok(())
    }

    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError>;

    async fn trigger_stop(&self) -> Result<(), PlayerError>;

    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError>;

    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError>;

    async fn trigger_resume(&self) -> Result<(), PlayerError>;

    /// # Errors
    ///
    /// * If failed to access the player status
    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError>;

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

pub static SERVICE_PORT: LazyLock<RwLock<Option<u16>>> = LazyLock::new(|| RwLock::new(None));

/// # Panics
///
/// * If the `SERVICE_PORT` `RwLock` is poisoned
pub fn set_service_port(service_port: u16) {
    SERVICE_PORT.write().unwrap().replace(service_port);
}

type PlaybackEventCallback = fn(&UpdateSession, &Playback);

static PLAYBACK_EVENT_LISTENERS: LazyLock<Arc<RwLock<Vec<PlaybackEventCallback>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// # Panics
///
/// * If the `PLAYBACK_EVENT_LISTENERS` `RwLock` is poisoned
pub fn on_playback_event(listener: PlaybackEventCallback) {
    PLAYBACK_EVENT_LISTENERS.write().unwrap().push(listener);
}

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
    quality: PlaybackQuality,
) -> Result<PlayableTrack, PlayerError> {
    log::trace!("track_to_playable_file track={track:?} quality={quality:?}");

    let mut hint = Hint::new();

    let file = track.file.clone().unwrap();
    let path = Path::new(&file);

    // Provide the file extension as a hint.
    if let Some(extension) = path.extension() {
        if let Some(extension_str) = extension.to_str() {
            hint.with_extension(extension_str);
        }
    }

    #[allow(clippy::match_wildcard_for_single_variants)]
    let same_source = match quality.format {
        AudioFormat::Source => true,
        #[allow(unreachable_patterns)]
        _ => track.format.is_none_or(|format| format == quality.format),
    };

    let source: Box<dyn MediaSource> = if same_source {
        Box::new(File::open(path)?)
    } else {
        #[allow(unused_mut)]
        let mut signal_chain = SignalChain::new();

        match quality.format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => {
                use moosicbox_audio_output::encoder::aac::AacEncoder;
                log::debug!("Encoding playback with AacEncoder");
                let mut hint = Hint::new();
                hint.with_extension("m4a");
                signal_chain = signal_chain
                    .add_encoder_step(|| Box::new(AacEncoder::new()))
                    .with_hint(hint);
            }
            #[cfg(feature = "flac")]
            AudioFormat::Flac => {
                use moosicbox_audio_output::encoder::flac::FlacEncoder;
                log::debug!("Encoding playback with FlacEncoder");
                let mut hint = Hint::new();
                hint.with_extension("flac");
                signal_chain = signal_chain
                    .add_encoder_step(|| Box::new(FlacEncoder::new()))
                    .with_hint(hint);
            }
            #[cfg(feature = "mp3")]
            AudioFormat::Mp3 => {
                use moosicbox_audio_output::encoder::mp3::Mp3Encoder;
                log::debug!("Encoding playback with Mp3Encoder");
                let mut hint = Hint::new();
                hint.with_extension("mp3");
                signal_chain = signal_chain
                    .add_encoder_step(|| Box::new(Mp3Encoder::new()))
                    .with_hint(hint);
            }
            #[cfg(feature = "opus")]
            AudioFormat::Opus => {
                use moosicbox_audio_output::encoder::opus::OpusEncoder;
                log::debug!("Encoding playback with OpusEncoder");
                let mut hint = Hint::new();
                hint.with_extension("opus");
                signal_chain = signal_chain
                    .add_encoder_step(|| Box::new(OpusEncoder::new()))
                    .with_hint(hint);
            }
            #[allow(unreachable_patterns)]
            _ => {
                moosicbox_assert::die!("Invalid format {}", quality.format);
            }
        }

        log::trace!("track_to_playable_file: getting file at path={path:?}");
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
    quality: PlaybackQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    track_id_to_playable_stream(&track.id, track.api_source, quality, player_source, abort).await
}

#[allow(unused)]
async fn track_id_to_playable_stream(
    track_id: &Id,
    source: ApiSource,
    quality: PlaybackQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    let (url, headers) = get_track_url(track_id, source, player_source, quality, false).await?;

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
        #[cfg(feature = "flac")]
        {
            quality.format == moosicbox_music_models::AudioFormat::Flac
        },
        #[cfg(not(feature = "flac"))]
        false,
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
    quality: PlaybackQuality,
    player_source: &PlayerSource,
    abort: CancellationToken,
) -> Result<PlayableTrack, PlayerError> {
    log::trace!(
        "track_or_id_to_playable playback_type={playback_type:?} track={track:?} quality={quality:?}"
    );
    Ok(match (playback_type, track.api_source) {
        (PlaybackType::File | PlaybackType::Default, ApiSource::Library) => {
            track_to_playable_file(track, quality).await?
        }
        _ => track_to_playable_stream(track, quality, player_source, abort).await?,
    })
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
            tokio::time::sleep(retry_options.unwrap().retry_delay).await;
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

/// # Panics
///
/// * If the `PLAYBACK_EVENT_LISTENERS` `RwLock` is poisoned
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn send_playback_event(update: &UpdateSession, playback: &Playback) {
    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(update, playback);
    }
}
