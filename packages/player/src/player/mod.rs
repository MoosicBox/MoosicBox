use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use atomic_float::AtomicF64;
use crossbeam_channel::SendError;
use futures::{StreamExt as _, TryStreamExt as _};
use lazy_static::lazy_static;
use local_ip_address::local_ip;
use moosicbox_core::{
    sqlite::{
        db::{get_album_tracks, get_session_playlist, get_tracks, DbError},
        models::{
            qobuz::QobuzTrack, tidal::TidalTrack, ApiSource, LibraryTrack, ToApi, Track,
            TrackApiSource, UpdateSession, UpdateSessionPlaylistTrack,
        },
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::Database;
use moosicbox_json_utils::{serde_json::ToValue as _, ParseError};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use moosicbox_symphonia_player::{
    media_sources::bytestream_source::ByteStreamSource,
    signal_chain::{SignalChain, SignalChainError},
    PlaybackError,
};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use symphonia::core::{io::MediaSource, probe::Hint};
use thiserror::Error;
use tokio::runtime::{self, Runtime};
use tokio_util::{
    codec::{BytesCodec, FramedRead},
    sync::CancellationToken,
};

#[cfg(feature = "local")]
pub mod local;

lazy_static! {
    pub static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

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
    IO(#[from] std::io::Error),
    #[error("Format not supported: {0:?}")]
    UnsupportedFormat(AudioFormat),
    #[error(transparent)]
    PlaybackError(#[from] moosicbox_symphonia_player::PlaybackError),
    #[error("Track fetch failed: {0}")]
    TrackFetchFailed(i32),
    #[error("Album fetch failed: {0}")]
    AlbumFetchFailed(i32),
    #[error("Track not found: {0}")]
    TrackNotFound(i32),
    #[error("Track not locally stored: {0}")]
    TrackNotLocal(i32),
    #[error("No players playing")]
    NoPlayersPlaying,
    #[error("Position out of bounds: {0}")]
    PositionOutOfBounds(i32),
    #[error("Playback not playing: {0}")]
    PlaybackNotPlaying(usize),
    #[error("Playback already playing: {0}")]
    PlaybackAlreadyPlaying(usize),
    #[error("Invalid Playback Type")]
    InvalidPlaybackType,
    #[error("Invalid session with id {session_id}: {message}")]
    InvalidSession { session_id: i32, message: String },
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
    pub id: usize,
    pub session_id: Option<usize>,
    pub session_playlist_id: Option<usize>,
    pub tracks: Vec<TrackOrId>,
    pub playing: bool,
    pub position: u16,
    pub quality: PlaybackQuality,
    pub progress: f64,
    pub volume: Arc<AtomicF64>,
    pub abort: CancellationToken,
}

impl Playback {
    pub fn new(
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        volume: AtomicF64,
        quality: PlaybackQuality,
        session_id: Option<usize>,
        session_playlist_id: Option<usize>,
    ) -> Playback {
        Playback {
            id: thread_rng().gen::<usize>(),
            session_id,
            session_playlist_id,
            tracks,
            playing: false,
            position: position.unwrap_or_default(),
            quality,
            progress: 0.0,
            volume: Arc::new(volume),
            abort: CancellationToken::new(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayback {
    pub track_ids: Vec<i32>,
    pub playing: bool,
    pub position: u16,
    pub seek: f64,
}

impl ToApi<ApiPlayback> for Playback {
    fn to_api(self) -> ApiPlayback {
        ApiPlayback {
            track_ids: self.tracks.iter().map(|t| t.id()).collect(),
            playing: self.playing,
            position: self.position,
            seek: self.progress,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlaybackStatus {
    pub active_playbacks: Option<ApiPlayback>,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    pub playback_id: usize,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackOrId {
    Track(Box<Track>),
    Id(i32, ApiSource),
}

impl TrackOrId {
    pub fn api_source(&self) -> ApiSource {
        match self {
            TrackOrId::Track(track) => match track.as_ref() {
                Track::Library(_) => ApiSource::Library,
                Track::Tidal(_) => ApiSource::Tidal,
                Track::Qobuz(_) => ApiSource::Qobuz,
            },
            TrackOrId::Id(_id, source) => *source,
        }
    }

    pub fn track_source(&self) -> TrackApiSource {
        match self {
            TrackOrId::Track(track) => match track.as_ref() {
                Track::Library(track) => track.source,
                Track::Tidal(_) => TrackApiSource::Tidal,
                Track::Qobuz(_) => TrackApiSource::Qobuz,
            },
            TrackOrId::Id(_id, source) => match source {
                ApiSource::Library => TrackApiSource::Local,
                ApiSource::Tidal => TrackApiSource::Tidal,
                ApiSource::Qobuz => TrackApiSource::Qobuz,
            },
        }
    }

    pub fn track(&self) -> Option<&Track> {
        match self {
            TrackOrId::Track(track) => Some(track),
            TrackOrId::Id(_id, _) => None,
        }
    }

    pub fn id(&self) -> i32 {
        match self {
            TrackOrId::Track(track) => match track.as_ref() {
                Track::Library(track) => track.id,
                Track::Tidal(track) => track.id as i32,
                Track::Qobuz(track) => track.id as i32,
            },
            TrackOrId::Id(id, _) => *id,
        }
    }

    pub fn to_id(&self) -> TrackOrId {
        match self {
            TrackOrId::Track(track) => match track.as_ref() {
                Track::Library(track) => TrackOrId::Id(track.id, ApiSource::Library),
                Track::Tidal(track) => TrackOrId::Id(track.id as i32, ApiSource::Library),
                Track::Qobuz(track) => TrackOrId::Id(track.id as i32, ApiSource::Library),
            },
            TrackOrId::Id(_, _) => self.clone(),
        }
    }
}

pub async fn get_track_url(
    track_id: u64,
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
        }

        serializer.finish()
    };

    let query_string = format!("?{}", query_params);

    let url = match api_source {
        ApiSource::Library => Ok(format!("{host}/track{query_string}")),
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
                .ok_or(PlayerError::TrackFetchFailed(track_id as i32))
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
    }?;

    Ok((url, headers))
}

impl From<TrackOrId> for UpdateSessionPlaylistTrack {
    fn from(value: TrackOrId) -> Self {
        UpdateSessionPlaylistTrack {
            id: value.id() as u64,
            r#type: value.api_source(),
            data: value
                .track()
                .map(|t| serde_json::to_string(t).expect("Failed to stringify track")),
        }
    }
}

pub async fn get_session_playlist_id_from_session_id(
    db: &dyn Database,
    session_id: Option<usize>,
) -> Result<Option<usize>, PlayerError> {
    Ok(if let Some(session_id) = session_id {
        Some(
            get_session_playlist(db, session_id as i32)
                .await?
                .ok_or(PlayerError::Db(DbError::InvalidRequest))?
                .id as usize,
        )
    } else {
        None
    })
}

pub struct PlayableTrack {
    pub track_id: i32,
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
    pub max_retry_count: u32,
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
pub trait Player {
    async fn init_from_session(
        &mut self,
        db: &dyn Database,
        session_id: i32,
    ) -> Result<(), PlayerError> {
        log::trace!("Searching for existing session id {}", session_id);
        if let Ok(session) = moosicbox_core::sqlite::db::get_session(db, session_id).await {
            if let Some(session) = session {
                log::debug!("Got session {session:?}");
                if let Err(err) = self
                    .update_playback(
                        None,
                        None,
                        Some(session.playing),
                        session.position.map(|x| x.try_into().unwrap()),
                        session.seek.map(std::convert::Into::into),
                        session.volume,
                        Some(
                            session
                                .playlist
                                .tracks
                                .iter()
                                .map(|x| {
                                    TrackOrId::Id(x.track_id().try_into().unwrap(), x.api_source())
                                })
                                .collect::<Vec<_>>(),
                        ),
                        None,
                        Some(session.id.try_into().unwrap()),
                        Some(session.playlist.id.try_into().unwrap()),
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
        db: &dyn Database,
        session_id: Option<usize>,
        album_id: i32,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        let tracks = {
            get_album_tracks(db, album_id as u64)
                .await
                .map_err(|e| {
                    log::error!("Failed to fetch album tracks: {e:?}");
                    PlayerError::AlbumFetchFailed(album_id)
                })?
                .into_iter()
                .map(Track::Library)
                .map(Box::new)
                .map(TrackOrId::Track)
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
            retry_options,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn play_track(
        &self,
        db: &dyn Database,
        session_id: Option<usize>,
        track: TrackOrId,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        self.play_tracks(
            db,
            session_id,
            vec![track],
            None,
            seek,
            volume,
            quality,
            retry_options,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn play_tracks(
        &self,
        db: &dyn Database,
        session_id: Option<usize>,
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Ok(playback) = self.get_playback() {
            log::debug!("Stopping existing playback {}", playback.id);
            self.stop()?;
        }

        let tracks = {
            let library_tracks = {
                get_tracks(
                    db,
                    Some(
                        &tracks
                            .iter()
                            .filter(|t| t.api_source() == ApiSource::Library)
                            .map(|t| t.id() as u64)
                            .collect::<Vec<_>>(),
                    ),
                )
                .await?
            };

            tracks
                .iter()
                .map(|track| match track {
                    TrackOrId::Id(track_id, source) => match *source {
                        ApiSource::Library => {
                            log::debug!("Fetching track {track_id}",);
                            let track = library_tracks
                                .iter()
                                .find(|t| t.id == *track_id)
                                .expect("Track doesn't exist");
                            log::debug!("Got track {track:?}");
                            TrackOrId::Track(Box::new(Track::Library(track.clone())))
                        }
                        ApiSource::Tidal => TrackOrId::Track(Box::new(Track::Tidal(TidalTrack {
                            id: *track_id as u64,
                            ..Default::default()
                        }))),
                        ApiSource::Qobuz => TrackOrId::Track(Box::new(Track::Qobuz(QobuzTrack {
                            id: *track_id as u64,
                            ..Default::default()
                        }))),
                    },
                    TrackOrId::Track(track) => TrackOrId::Track(track.clone()),
                })
                .collect()
        };

        let playback = Playback::new(
            tracks,
            position,
            AtomicF64::new(volume.unwrap_or(1.0)),
            quality,
            session_id,
            get_session_playlist_id_from_session_id(db, session_id).await?,
        );

        self.play_playback(playback, seek, retry_options)
    }

    fn play_playback(
        &self,
        playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError>;

    async fn start_playback(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError>;

    fn stop_track(&self) -> Result<PlaybackStatus, PlayerError> {
        log::debug!("stop_track called");
        let playback = self.stop()?;

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    fn stop(&self) -> Result<Playback, PlayerError>;

    async fn seek_track(
        &self,
        seek: f64,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::debug!("seek_track seek={seek}");
        let playback = self.stop()?;
        let playback_id = playback.id;
        self.play_playback(playback, Some(seek), retry_options)?;

        Ok(PlaybackStatus {
            success: true,
            playback_id,
        })
    }

    async fn next_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position + 1 >= playback.tracks.len() as u16 {
            return Err(PlayerError::PositionOutOfBounds(
                playback.position as i32 + 1,
            ));
        }

        self.update_playback(
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
            retry_options,
        )
        .await
    }

    async fn previous_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Playing next track seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position == 0 {
            return Err(PlayerError::PositionOutOfBounds(-1));
        }

        self.update_playback(
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
            retry_options,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_playback(
        &self,
        play: Option<bool>,
        stop: Option<bool>,
        playing: Option<bool>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        tracks: Option<Vec<TrackOrId>>,
        quality: Option<PlaybackQuality>,
        session_id: Option<usize>,
        session_playlist_id: Option<usize>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError>;

    async fn pause_playback(&self) -> Result<PlaybackStatus, PlayerError>;

    async fn resume_playback(
        &self,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Resuming playback");
        let mut playback = self.get_playback()?;

        let id = playback.id;

        if playback.playing {
            return Err(PlayerError::PlaybackAlreadyPlaying(id));
        }

        let seek = Some(playback.progress);

        playback.playing = true;
        playback.abort = CancellationToken::new();

        self.play_playback(playback, seek, retry_options)
    }

    async fn track_to_playable_file(
        &self,
        track: &LibraryTrack,
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
                    use moosicbox_symphonia_player::output::encoder::aac::encoder::AacEncoder;
                    let mut hint = Hint::new();
                    hint.with_extension("m4a");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(AacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "flac")]
                AudioFormat::Flac => {
                    use moosicbox_symphonia_player::output::encoder::flac::encoder::FlacEncoder;
                    let mut hint = Hint::new();
                    hint.with_extension("flac");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(FlacEncoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "mp3")]
                AudioFormat::Mp3 => {
                    use moosicbox_symphonia_player::output::encoder::mp3::encoder::Mp3Encoder;
                    let mut hint = Hint::new();
                    hint.with_extension("mp3");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(Mp3Encoder::new()))
                        .with_hint(hint);
                }
                #[cfg(feature = "opus")]
                AudioFormat::Opus => {
                    use moosicbox_symphonia_player::output::encoder::opus::encoder::OpusEncoder;
                    let mut hint = Hint::new();
                    hint.with_extension("opus");
                    signal_chain = signal_chain
                        .add_encoder_step(|| Box::new(OpusEncoder::new()))
                        .with_hint(hint);
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }

            let file = tokio::fs::File::open(path.to_path_buf()).await?;

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
                Err(SignalChainError::Playback(err)) => {
                    return Err(PlayerError::PlaybackError(match err {
                        moosicbox_symphonia_player::unsync::PlaybackError::AudioOutput(err) => {
                            PlaybackError::AudioOutput(err)
                        }
                        moosicbox_symphonia_player::unsync::PlaybackError::Symphonia(err) => {
                            PlaybackError::Symphonia(err)
                        }
                    }));
                }
                Err(SignalChainError::Empty) => unreachable!("Empty signal chain"),
            }
        };

        Ok(PlayableTrack {
            track_id: track.id,
            source,
            hint,
        })
    }

    async fn track_or_id_to_playable_stream(
        &self,
        track_or_id: &TrackOrId,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        match track_or_id {
            TrackOrId::Id(id, source) => {
                self.track_id_to_playable_stream(*id, *source, quality)
                    .await
            }
            TrackOrId::Track(track) => self.track_to_playable_stream(track, quality).await,
        }
    }

    async fn track_to_playable_stream(
        &self,
        track: &Track,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        self.track_id_to_playable_stream(
            match track {
                Track::Library(track) => track.id,
                Track::Tidal(track) => track.id as i32,
                Track::Qobuz(track) => track.id as i32,
            },
            match track {
                Track::Library(_) => ApiSource::Library,
                Track::Tidal(_) => ApiSource::Tidal,
                Track::Qobuz(_) => ApiSource::Qobuz,
            },
            quality,
        )
        .await
    }

    async fn track_id_to_playable_stream(
        &self,
        track_id: i32,
        source: ApiSource,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError>;

    async fn track_or_id_to_playable(
        &self,
        playback_type: PlaybackType,
        track_or_id: &TrackOrId,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        log::trace!("track_or_id_to_playable playback_type={playback_type:?} track_or_id={track_or_id:?} quality={quality:?}");
        Ok(match playback_type {
            PlaybackType::File => match track_or_id.clone() {
                TrackOrId::Id(_id, _) => return Err(PlayerError::InvalidPlaybackType),
                TrackOrId::Track(track) => match *track {
                    Track::Library(track) => self.track_to_playable_file(&track, quality).await?,
                    Track::Tidal(track) => {
                        self.track_to_playable_stream(&Track::Tidal(track), quality)
                            .await?
                    }
                    Track::Qobuz(track) => {
                        self.track_to_playable_stream(&Track::Qobuz(track), quality)
                            .await?
                    }
                },
            },
            PlaybackType::Stream => {
                self.track_or_id_to_playable_stream(track_or_id, quality)
                    .await?
            }
            PlaybackType::Default => match track_or_id.clone() {
                TrackOrId::Id(id, source) => {
                    self.track_id_to_playable_stream(id, source, quality)
                        .await?
                }
                TrackOrId::Track(track) => match *track {
                    Track::Library(track) => self.track_to_playable_file(&track, quality).await?,
                    Track::Tidal(track) => {
                        self.track_to_playable_stream(&Track::Tidal(track), quality)
                            .await?
                    }
                    Track::Qobuz(track) => {
                        self.track_to_playable_stream(&Track::Qobuz(track), quality)
                            .await?
                    }
                },
            },
        })
    }

    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError>;

    fn get_playback(&self) -> Result<Playback, PlayerError>;
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
        Some(current.position as i32)
    } else {
        None
    };
    let seek = if current.progress as usize != previous.progress as usize {
        has_change = true;
        Some(current.progress as i32 as f64)
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
        Some(moosicbox_core::sqlite::models::UpdateSessionPlaylist {
            session_playlist_id: current
                .session_playlist_id
                .map(|id| id as i32)
                .unwrap_or(-1),
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
        volume={volume:?}\n\t\
        playlist={playlist:?}\
        "
    );

    let update = UpdateSession {
        session_id: session_id as i32,
        play: None,
        stop: None,
        name: None,
        active: None,
        playing,
        position,
        seek,
        volume,
        playlist,
    };

    send_playback_event(&update, current)
}

pub fn send_playback_event(update: &UpdateSession, playback: &Playback) {
    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(update, playback);
    }
}
