#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

use ::symphonia::core::{io::MediaSource, probe::Hint};
use actix_web::Result;
use async_trait::async_trait;
use atomic_float::AtomicF64;
use flume::{bounded, Receiver, SendError};
use futures::{Future, StreamExt as _, TryStreamExt as _};
use local_ip_address::local_ip;
use moosicbox_audio_decoder::media_sources::{
    bytestream_source::ByteStreamSource, remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_core::sqlite::models::ApiSource;
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Id, ToApi, TrackApiSource},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::Database;
use moosicbox_json_utils::{serde_json::ToValue as _, ParseError};
use moosicbox_music_api::MusicApi;
use moosicbox_session::{
    get_session, get_session_playlist,
    models::{UpdateSession, UpdateSessionPlaylist, UpdateSessionPlaylistTrack},
};
use moosicbox_stream_utils::{
    remote_bytestream::RemoteByteStream, stalled_monitor::StalledReadMonitor,
};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(#[from] SendError<()>),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Db(#[from] DbError),
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
}

impl std::fmt::Debug for PlayableTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayableTrack")
            .field("track_id", &self.track_id)
            .field("source", &"{{source}}")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Playback {
    pub id: u64,
    pub session_id: Option<u64>,
    pub session_playlist_id: Option<u64>,
    pub tracks: Vec<Track>,
    pub playing: bool,
    pub position: u16,
    pub quality: PlaybackQuality,
    pub progress: f64,
    pub volume: Arc<AtomicF64>,
    pub audio_zone_id: Option<u64>,
    pub abort: CancellationToken,
}

impl Playback {
    pub fn new(
        tracks: Vec<Track>,
        position: Option<u16>,
        volume: AtomicF64,
        quality: PlaybackQuality,
        session_id: Option<u64>,
        session_playlist_id: Option<u64>,
        audio_zone_id: Option<u64>,
    ) -> Playback {
        Playback {
            id: thread_rng().gen::<u64>(),
            session_id,
            session_playlist_id,
            tracks,
            playing: false,
            position: position.unwrap_or_default(),
            quality,
            progress: 0.0,
            volume: Arc::new(volume),
            audio_zone_id,
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

impl ToApi<ApiPlayback> for Playback {
    fn to_api(self) -> ApiPlayback {
        ApiPlayback {
            track_ids: self.tracks.iter().map(|t| t.id.to_string()).collect(),
            playing: self.playing,
            position: self.position,
            seek: self.progress,
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

#[derive(Debug, Clone)]
pub struct Track {
    pub id: Id,
    pub source: ApiSource,
    pub data: Option<Value>,
}

impl Track {
    pub fn track_source(&self) -> TrackApiSource {
        match self.source {
            ApiSource::Library => self
                .data
                .as_ref()
                .and_then(|x| x.get("source"))
                .map(|x| serde_json::from_value(x.clone()))
                .transpose()
                .expect("Missing source")
                .unwrap_or(TrackApiSource::Local),
            ApiSource::Tidal => TrackApiSource::Tidal,
            ApiSource::Qobuz => TrackApiSource::Qobuz,
            ApiSource::Yt => TrackApiSource::Yt,
        }
    }
}

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
        } => (host.to_string(), query, headers.to_owned()),
        PlayerSource::Local => {
            let ip = if use_local_network_ip {
                local_ip().map(|x| x.to_string()).unwrap_or_else(|e| {
                    log::warn!("Failed to get local ip address: {e:?}");
                    "127.0.0.1".to_string()
                })
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

        match api_source {
            ApiSource::Library => {
                if quality.format != AudioFormat::Source {
                    serializer.append_pair("format", quality.format.as_ref());
                }
            }
            ApiSource::Tidal => {
                serializer.append_pair("audioQuality", "HIGH");
            }
            ApiSource::Qobuz => {
                serializer.append_pair("audioQuality", "LOW");
            }
            ApiSource::Yt => {
                serializer.append_pair("audioQuality", "LOW");
            }
        }

        serializer.finish()
    };

    let query_string = format!("?{}", query_params);

    let url = match api_source {
        ApiSource::Library => Ok(format!("{host}/files/track{query_string}")),
        ApiSource::Tidal => {
            let url = format!("{host}/tidal/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            CLIENT
                .get(url)
                .send()
                .await?
                .json::<Value>()
                .await?
                .to_value::<Vec<String>>("urls")?
                .first()
                .cloned()
                .ok_or(PlayerError::TrackFetchFailed(track_id.to_string()))
        }
        ApiSource::Qobuz => {
            let url = format!("{host}/qobuz/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            Ok(CLIENT
                .get(url)
                .send()
                .await?
                .json::<Value>()
                .await?
                .to_value::<String>("url")?)
        }
        ApiSource::Yt => {
            let url = format!("{host}/yt/track/url{query_string}");
            log::debug!("Fetching track file url from {url}");

            Ok(CLIENT
                .get(url)
                .send()
                .await?
                .json::<Value>()
                .await?
                .to_value::<String>("url")?)
        }
    }?;

    Ok((url, headers))
}

impl From<Track> for UpdateSessionPlaylistTrack {
    fn from(value: Track) -> Self {
        UpdateSessionPlaylistTrack {
            id: value.id.to_string(),
            r#type: value.source,
            data: value
                .data
                .as_ref()
                .map(|t| serde_json::to_string(t).expect("Failed to stringify track")),
        }
    }
}

pub async fn get_session_playlist_id_from_session_id(
    db: &dyn Database,
    session_id: Option<u64>,
) -> Result<Option<u64>, PlayerError> {
    Ok(if let Some(session_id) = session_id {
        Some(
            get_session_playlist(db, session_id)
                .await?
                .ok_or(PlayerError::Db(DbError::InvalidRequest))?
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

#[async_trait]
pub trait Player: Clone + Send + 'static {
    fn active_playback_write(&self) -> RwLockWriteGuard<'_, Option<Playback>>;
    async fn receiver_write(&self) -> tokio::sync::RwLockWriteGuard<'_, Option<Receiver<()>>>;

    async fn init_from_session(
        &self,
        db: &dyn Database,
        init: &UpdateSession,
    ) -> Result<(), PlayerError> {
        let session_id = init.session_id;
        log::trace!("Searching for existing session id {}", session_id);
        if let Ok(session) = get_session(db, session_id).await {
            if let Some(session) = session {
                log::debug!("Got session {session:?}");
                let track_ids = session
                    .playlist
                    .tracks
                    .iter()
                    .map(|x| x.track_id())
                    .collect::<Vec<_>>();
                let tracks = moosicbox_library::db::get_tracks(db, Some(&track_ids)).await?;
                if let Err(err) = self
                    .update_playback(
                        false,
                        None,
                        None,
                        init.playing.or(Some(session.playing)),
                        init.position.or(session.position),
                        init.seek.map(std::convert::Into::into),
                        init.volume.or(session.volume),
                        Some(
                            session
                                .playlist
                                .tracks
                                .iter()
                                .map(|x| Track {
                                    id: x.track_id(),
                                    source: x.api_source(),
                                    data: if x.api_source() == ApiSource::Library {
                                        tracks
                                            .iter()
                                            .find(|t| {
                                                let track_id: u64 = x.track_id().into();
                                                t.id == track_id
                                            })
                                            .and_then(|x| serde_json::to_value(x).ok())
                                    } else {
                                        x.data()
                                    },
                                })
                                .collect::<Vec<_>>(),
                        ),
                        None,
                        Some(session.id),
                        Some(session.playlist.id),
                        Some(session.audio_zone_id),
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
            } else {
                log::debug!("No session with id {}", session_id);
            }
        } else {
            return Err(PlayerError::InvalidSession {
                session_id,
                message: format!("Failed to get session with id {}", session_id),
            });
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn play_album(
        &self,
        api: &dyn MusicApi,
        db: &dyn Database,
        session_id: Option<u64>,
        album_id: &Id,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        audio_zone_id: Option<u64>,
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
                .into_iter()
                .map(|x| Track {
                    id: x.id.to_owned(),
                    source: ApiSource::Library,
                    data: Some(serde_json::to_value(x).unwrap()),
                })
                .collect()
        };

        self.play_tracks(
            db,
            session_id,
            tracks,
            position,
            seek,
            volume,
            quality,
            audio_zone_id,
            retry_options,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn play_track(
        &self,
        db: &dyn Database,
        session_id: Option<u64>,
        track: Track,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        audio_zone_id: Option<u64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        self.play_tracks(
            db,
            session_id,
            vec![track],
            None,
            seek,
            volume,
            quality,
            audio_zone_id,
            retry_options,
        )
        .await
    }

    async fn handle_retry<
        T,
        E: std::fmt::Debug + Into<PlayerError>,
        F: Future<Output = Result<T, E>> + Send,
    >(
        &self,
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
                    if let PlayerError::Cancelled = e {
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
                    } else {
                        log::debug!("No retry options");
                        break Err(e);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn play_tracks(
        &self,
        db: &dyn Database,
        session_id: Option<u64>,
        tracks: Vec<Track>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        audio_zone_id: Option<u64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        if let Some(playback) = self.get_playback() {
            log::debug!("Stopping existing playback {}", playback.id);
            self.stop(retry_options).await?;
        }

        let playback = Playback::new(
            tracks,
            position,
            AtomicF64::new(volume.unwrap_or(1.0)),
            quality,
            session_id,
            get_session_playlist_id_from_session_id(db, session_id).await?,
            audio_zone_id,
        );

        self.active_playback_write().replace(playback);

        self.play_playback(seek, retry_options).await
    }

    async fn before_play_playback(&self, _seek: Option<f64>) -> Result<(), PlayerError> {
        Ok(())
    }

    async fn play_playback(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        self.before_play_playback(seek).await?;

        let mut playback = self.get_playback().ok_or(PlayerError::NoPlayersPlaying)?;
        log::info!("play_playback: playback={playback:?}");

        if playback.tracks.is_empty() {
            log::debug!("No tracks to play for {playback:?}");
            return Ok(());
        }

        let (tx, rx) = bounded(1);

        self.receiver_write().await.replace(rx);

        let old = playback.clone();

        playback.playing = true;

        trigger_playback_event(&playback, &old);

        self.active_playback_write().replace(playback.clone());

        let player = self.clone();

        log::debug!(
            "Playing playback: position={} tracks={:?}",
            playback.position,
            playback.tracks.iter().map(|t| &t.id).collect::<Vec<_>>()
        );

        moosicbox_task::spawn("player: Play playback", async move {
            let mut seek = seek;
            let mut playback = playback.clone();
            let abort = playback.abort.clone();

            while playback.playing && (playback.position as usize) < playback.tracks.len() {
                let track_or_id = &playback.tracks[playback.position as usize];
                log::debug!("play_playback: track={track_or_id:?} seek={seek:?}");

                let seek = if seek.is_some() { seek.take() } else { None };

                tokio::select! {
                    _ = abort.cancelled() => {
                        log::debug!("Playback cancelled");
                        return Err(PlayerError::Cancelled);
                    }
                    resp = player.play(seek, retry_options) => {
                        if let Err(err) = resp {
                            log::error!("Playback error occurred: {err:?}");

                            {
                                let mut binding = player.active_playback_write();
                                let active = binding.as_mut().unwrap();
                                let old = active.clone();
                                active.playing = false;
                                trigger_playback_event(active, &old);
                            }

                            tx.send_async(()).await?;
                            return Err(err);
                        }
                    }
                }

                log::debug!("play_playback: playback finished track={track_or_id:?}");

                let mut binding = player.active_playback_write();
                let active = binding.as_mut().unwrap();

                if ((active.position + 1) as usize) >= active.tracks.len() {
                    log::debug!("Playback position at end of tracks. Breaking");
                    break;
                }

                let old = active.clone();
                active.position += 1;
                active.progress = 0.0;
                trigger_playback_event(active, &old);

                playback = active.clone();
            }

            log::debug!(
                "Finished playback on all tracks. playing={} position={} len={}",
                playback.playing,
                playback.position,
                playback.tracks.len()
            );

            {
                let mut binding = player.active_playback_write();
                let active = binding.as_mut().unwrap();
                let old = active.clone();
                active.playing = false;

                trigger_playback_event(active, &old);
            }

            tx.send_async(()).await?;

            Ok::<_, PlayerError>(0)
        });

        Ok(())
    }

    async fn play(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("play: seek={seek:?}");

        self.handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.trigger_play(seek).await }
            }
        })
        .await?;

        Ok(())
    }

    #[doc(hidden)]
    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError>;

    async fn stop(&self, retry_options: Option<PlaybackRetryOptions>) -> Result<(), PlayerError> {
        log::debug!("stop: Stopping playback");

        self.handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.trigger_stop().await }
            }
        })
        .await?;

        Ok(())
    }

    #[doc(hidden)]
    async fn trigger_stop(&self) -> Result<(), PlayerError>;

    async fn seek(
        &self,
        seek: f64,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::debug!("seek: seek={seek:?}");

        self.handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.trigger_seek(seek).await }
            }
        })
        .await?;

        Ok(())
    }

    #[doc(hidden)]
    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError>;

    async fn next_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = self.get_playback().ok_or(PlayerError::NoPlayersPlaying)?;

        if playback.position + 1 >= playback.tracks.len() as u16 {
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

    async fn previous_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = self.get_playback().ok_or(PlayerError::NoPlayersPlaying)?;

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

    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_playback(
        &self,
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
        session_playlist_id: Option<u64>,
        audio_zone_id: Option<Option<u64>>,
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
            audio_zone_id={audio_zone_id:?}\n\t\
            trigger_event={trigger_event}\
            "
        );

        self.before_update_playback().await?;

        let original = self.get_playback();

        if let Some(original) = &original {
            log::trace!("update_playback: existing playback={original:?}");
        }

        let original = original.unwrap_or(Playback::new(
            tracks.clone().unwrap_or_default(),
            position,
            AtomicF64::new(volume.unwrap_or(1.0)),
            quality.unwrap_or_default(),
            session_id,
            session_playlist_id,
            audio_zone_id.flatten(),
        ));

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
            session_id: original.session_id,
            session_playlist_id: original.session_playlist_id,
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
            audio_zone_id: audio_zone_id.unwrap_or(original.audio_zone_id),
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
        self.active_playback_write().replace(playback.clone());

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

    async fn pause(&self, retry_options: Option<PlaybackRetryOptions>) -> Result<(), PlayerError> {
        log::debug!("pause: Pausing playback");

        self.handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.trigger_pause().await }
            }
        })
        .await?;

        Ok(())
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError>;

    async fn resume(&self, retry_options: Option<PlaybackRetryOptions>) -> Result<(), PlayerError> {
        log::debug!("resume: Resuming playback");

        self.handle_retry(retry_options, {
            let this = self.clone();

            move || {
                let this = this.clone();
                async move { this.trigger_resume().await }
            }
        })
        .await?;

        Ok(())
    }

    async fn trigger_resume(&self) -> Result<(), PlayerError>;

    async fn track_to_playable_file(
        &self,
        track: &moosicbox_core::sqlite::models::Track,
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

        let same_source = match quality.format {
            AudioFormat::Source => true,
            #[allow(unreachable_patterns)]
            _ => match track.format {
                Some(format) => format == quality.format,
                None => true,
            },
        };

        let source: Box<dyn MediaSource> = if same_source {
            Box::new(File::open(path)?)
        } else {
            #[allow(unused_mut)]
            let mut signal_chain = SignalChain::new();

            match quality.format {
                #[cfg(feature = "aac")]
                AudioFormat::Aac => {
                    log::debug!("Encoding playback with AacEncoder");
                    use moosicbox_audio_output::encoder::aac::AacEncoder;
                    let mut hint = Hint::new();
                    hint.with_extension("m4a");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(AacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "flac")]
                AudioFormat::Flac => {
                    log::debug!("Encoding playback with FlacEncoder");
                    use moosicbox_audio_output::encoder::flac::FlacEncoder;
                    let mut hint = Hint::new();
                    hint.with_extension("flac");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(FlacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "mp3")]
                AudioFormat::Mp3 => {
                    log::debug!("Encoding playback with Mp3Encoder");
                    use moosicbox_audio_output::encoder::mp3::Mp3Encoder;
                    let mut hint = Hint::new();
                    hint.with_extension("mp3");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(Mp3Encoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "opus")]
                AudioFormat::Opus => {
                    log::debug!("Encoding playback with OpusEncoder");
                    use moosicbox_audio_output::encoder::opus::OpusEncoder;
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
                        symphonia_unsync::PlaybackError::Symphonia(e) => {
                            PlaybackError::Symphonia(e)
                        }
                        symphonia_unsync::PlaybackError::Decode(e) => PlaybackError::Decode(e),
                    }));
                }
                Err(SignalChainError::Empty) => unreachable!("Empty signal chain"),
            }
        };

        Ok(PlayableTrack {
            track_id: track.id.to_owned(),
            source,
            hint,
        })
    }

    async fn track_to_playable_stream(
        &self,
        track: &Track,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        self.track_id_to_playable_stream(&track.id, track.source, quality)
            .await
    }

    async fn track_id_to_playable_stream(
        &self,
        track_id: &Id,
        source: ApiSource,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        let (url, headers) =
            get_track_url(track_id, source, self.get_source(), quality, false).await?;

        log::debug!("Fetching track bytes from url: {url}");

        let mut client = reqwest::Client::new().head(&url);

        if let Some(headers) = headers {
            for (key, value) in headers {
                client = client.header(key, value);
            }
        }

        let res = client.send().await.unwrap();
        let headers = res.headers();
        let size = headers
            .get("content-length")
            .map(|length| length.to_str().unwrap().parse::<u64>().unwrap());

        let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
            url,
            size,
            true,
            #[cfg(feature = "flac")]
            {
                quality.format == moosicbox_core::types::AudioFormat::Flac
            },
            #[cfg(not(feature = "flac"))]
            false,
            self.get_playback()
                .as_ref()
                .map(|p| p.abort.clone())
                .unwrap_or_default(),
        )
        .into();

        let mut hint = Hint::new();

        if let Some(Ok(content_type)) = headers
            .get(actix_web::http::header::CONTENT_TYPE.to_string())
            .map(|x| x.to_str())
        {
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

    async fn track_or_id_to_playable(
        &self,
        playback_type: PlaybackType,
        track: &Track,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        log::trace!("track_or_id_to_playable playback_type={playback_type:?} track={track:?} quality={quality:?}");
        Ok(match (playback_type, track.source) {
            (PlaybackType::File, ApiSource::Library)
            | (PlaybackType::Default, ApiSource::Library) => {
                self.track_to_playable_file(
                    &serde_json::from_value(
                        track
                            .data
                            .clone()
                            .ok_or(PlayerError::TrackNotFound(track.id.to_owned()))?,
                    )
                    .map_err(|e| {
                        moosicbox_assert::die_or_error!(
                            "Failed to parse track: {e:?} ({:?})",
                            track.data
                        );
                        PlayerError::TrackNotFound(track.id.to_owned())
                    })?,
                    quality,
                )
                .await?
            }
            (PlaybackType::File, ApiSource::Tidal)
            | (PlaybackType::File, ApiSource::Qobuz)
            | (PlaybackType::File, ApiSource::Yt)
            | (PlaybackType::Default, ApiSource::Tidal)
            | (PlaybackType::Default, ApiSource::Qobuz)
            | (PlaybackType::Default, ApiSource::Yt)
            | (PlaybackType::Stream, _) => self.track_to_playable_stream(track, quality).await?,
        })
    }

    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError>;

    fn get_playback(&self) -> Option<Playback>;

    fn get_source(&self) -> &PlayerSource;
}

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

pub static SERVICE_PORT: Lazy<RwLock<Option<u16>>> = Lazy::new(|| RwLock::new(None));

pub fn set_service_port(service_port: u16) {
    SERVICE_PORT.write().unwrap().replace(service_port);
}

type PlaybackEventCallback = fn(&UpdateSession, &Playback);

static PLAYBACK_EVENT_LISTENERS: Lazy<Arc<RwLock<Vec<PlaybackEventCallback>>>> =
    Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

pub fn on_playback_event(listener: PlaybackEventCallback) {
    PLAYBACK_EVENT_LISTENERS.write().unwrap().push(listener);
}

pub fn trigger_playback_event(current: &Playback, previous: &Playback) {
    if current.session_id.is_none() {
        return;
    }
    let session_id = current.session_id.unwrap();
    let mut has_change = false;

    let playing = if current.playing != previous.playing {
        has_change = true;
        Some(current.playing)
    } else {
        None
    };
    let position = if current.position != previous.position {
        has_change = true;
        Some(current.position)
    } else {
        None
    };
    let seek = if current.progress as usize != previous.progress as usize {
        has_change = true;
        Some(current.progress)
    } else {
        None
    };
    let current_volume = current.volume.load(std::sync::atomic::Ordering::SeqCst);
    let volume = if current_volume != previous.volume.load(std::sync::atomic::Ordering::SeqCst) {
        has_change = true;
        Some(current_volume)
    } else {
        None
    };
    let audio_zone_id = if current.audio_zone_id != previous.audio_zone_id {
        has_change = true;
        Some(current.audio_zone_id)
    } else {
        None
    };
    let quality = if current.quality != previous.quality {
        has_change = true;
        Some(current.quality)
    } else {
        None
    };
    let tracks = current
        .tracks
        .iter()
        .cloned()
        .map(|t| t.into())
        .collect::<Vec<_>>();
    let prev_tracks = previous
        .tracks
        .iter()
        .cloned()
        .map(|t| t.into())
        .collect::<Vec<_>>();
    let playlist = if tracks != prev_tracks {
        has_change = true;
        Some(UpdateSessionPlaylist {
            session_playlist_id: current.session_playlist_id.unwrap_or(0),
            tracks,
        })
    } else {
        None
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
        audio_zone_id={audio_zone_id:?}\n\t\
        playlist={playlist:?}\
        "
    );

    let update = UpdateSession {
        session_id,
        play: None,
        stop: None,
        name: None,
        active: None,
        playing,
        position,
        seek,
        volume,
        audio_zone_id,
        playlist,
        quality,
    };

    send_playback_event(&update, current)
}

pub fn send_playback_event(update: &UpdateSession, playback: &Playback) {
    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(update, playback);
    }
}
