use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, RwLock},
    u16, usize,
};

use crossbeam_channel::{bounded, Receiver, SendError, Sender};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album_tracks, get_track},
        models::{ToApi, Track, UpdateSession},
    },
};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStream,
    output::{AudioOutputError, AudioOutputHandler},
    PlaybackError, PlaybackHandle,
};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use symphonia::core::{
    io::{MediaSource, MediaSourceStream},
    probe::Hint,
};
use thiserror::Error;
use tokio::{
    runtime::{self, Runtime},
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use url::form_urlencoded;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(#[from] SendError<()>),
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
    pub tracks: Vec<TrackOrId>,
    pub playing: bool,
    pub position: u16,
    pub quality: PlaybackQuality,
    pub progress: f64,
    pub abort: CancellationToken,
}

impl Playback {
    pub fn new(
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        quality: PlaybackQuality,
        session_id: Option<usize>,
    ) -> Playback {
        Playback {
            id: thread_rng().gen::<usize>(),
            session_id,
            tracks,
            playing: true,
            position: position.unwrap_or_default(),
            quality,
            progress: 0.0,
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
    fn to_api(&self) -> ApiPlayback {
        ApiPlayback {
            track_ids: self
                .tracks
                .iter()
                .map(|t| match t {
                    TrackOrId::Track(track) => track.id,
                    TrackOrId::Id(id) => *id,
                })
                .collect(),
            playing: self.playing,
            position: self.position,
            seek: self.progress,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlaybackStatus {
    active_playbacks: Option<ApiPlayback>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    pub playback_id: usize,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub enum TrackOrId {
    Track(Track),
    Id(i32),
}

impl TrackOrId {
    pub fn track(&self) -> Option<&Track> {
        match self {
            TrackOrId::Track(track) => Some(track),
            TrackOrId::Id(_id) => None,
        }
    }

    pub fn id(&self) -> i32 {
        match self {
            TrackOrId::Track(track) => track.id,
            TrackOrId::Id(id) => *id,
        }
    }
}

pub struct PlayableTrack {
    pub track_id: i32,
    pub source: Box<dyn MediaSource>,
    pub hint: Hint,
}

#[derive(Copy, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaybackType {
    File,
    Stream,
    #[default]
    Default,
}

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AudioFormat {
    Aac,
    Flac,
    Mp3,
    Opus,
    #[default]
    Source,
}

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQuality {
    pub format: AudioFormat,
}

#[derive(Copy, Clone)]
pub struct PlaybackRetryOptions {
    pub max_retry_count: u32,
    pub retry_delay: std::time::Duration,
}

#[derive(Clone)]
pub enum PlayerSource {
    Local,
    Remote {
        host: String,
        query: Option<HashMap<String, String>>,
        headers: Option<HashMap<String, String>>,
    },
}

#[derive(Clone)]
pub struct Player {
    pub id: usize,
    playback_type: PlaybackType,
    source: PlayerSource,
    pub active_playback: Arc<RwLock<Option<Playback>>>,
    sender: Sender<()>,
    receiver: Receiver<()>,
}

impl Player {
    pub fn new(source: PlayerSource, playback_type: Option<PlaybackType>) -> Player {
        let (tx, rx) = bounded(1);

        Player {
            id: thread_rng().gen::<usize>(),
            playback_type: playback_type.unwrap_or_default(),
            source,
            active_playback: Arc::new(RwLock::new(None)),
            sender: tx,
            receiver: rx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play_album(
        &self,
        db: Db,
        session_id: Option<usize>,
        album_id: i32,
        position: Option<u16>,
        seek: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        let tracks = {
            let library = db.library.lock().unwrap();
            get_album_tracks(&library, album_id).map_err(|e| {
                error!("Failed to fetch album tracks: {e:?}");
                PlayerError::AlbumFetchFailed(album_id)
            })?
        };

        self.play_tracks(
            Some(db),
            session_id,
            tracks.into_iter().map(TrackOrId::Track).collect(),
            position,
            seek,
            quality,
            retry_options,
        )
    }

    pub fn play_track(
        &self,
        db: Option<Db>,
        session_id: Option<usize>,
        track: TrackOrId,
        seek: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        self.play_tracks(
            db,
            session_id,
            vec![track],
            None,
            seek,
            quality,
            retry_options,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play_tracks(
        &self,
        db: Option<Db>,
        session_id: Option<usize>,
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        seek: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Ok(playback) = self.get_playback() {
            debug!("Stopping existing playback {}", playback.id);
            self.stop()?;
        }

        let tracks = {
            let db = db.clone().expect("No DB set");
            let library = db.library.lock().unwrap();
            tracks
                .iter()
                .map(|track| match track {
                    TrackOrId::Id(track_id) => {
                        debug!("Fetching track {track_id}",);
                        let track = get_track(&library, *track_id)
                            .map_err(|e| {
                                error!("Failed to fetch track: {e:?}");
                                PlayerError::TrackFetchFailed(*track_id)
                            })
                            .expect("Failed to fetch track");
                        debug!("Got track {track:?}");
                        TrackOrId::Track(track.expect("Track doesn't exist"))
                    }
                    TrackOrId::Track(track) => TrackOrId::Track(track.clone()),
                })
                .collect()
        };
        let playback = Playback::new(tracks, position, quality, session_id);

        self.play_playback(playback, seek, retry_options)
    }

    pub fn play_playback(
        &self,
        playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Ok(playback) = self.get_playback() {
            debug!("Stopping existing playback {}", playback.id);
            self.stop()?;
        }

        self.active_playback
            .write()
            .unwrap()
            .replace(playback.clone());

        let player = self.clone();

        log::debug!(
            "Playing playback: position={} tracks={:?}",
            playback.position,
            playback.tracks.iter().map(|t| t.id()).collect::<Vec<_>>()
        );
        let playback_id = playback.id;

        RT.spawn(async move {
            let mut seek = seek;
            let mut playback = playback.clone();

            while (playback.position as usize) < playback.tracks.len() {
                let track_or_id = &playback.tracks[playback.position as usize];
                debug!("track {track_or_id:?} {seek:?}");

                let seek = if seek.is_some() { seek.take() } else { None };

                if let Err(err) = player.start_playback(seek, retry_options).await {
                    player.active_playback.write().unwrap().take();
                    return Err(err);
                }

                if playback.abort.is_cancelled() {
                    break;
                } else {
                    let mut binding = player.active_playback.write().unwrap();
                    let active = binding.as_mut().unwrap();
                    let old = active.clone();
                    if ((active.position + 1) as usize) < active.tracks.len() {
                        active.position += 1;
                        active.progress = 0.0;
                        trigger_playback_event(active, &old);
                    } else {
                        break;
                    }
                    playback = active.clone();
                }
            }

            player.active_playback.write().unwrap().take();
            player.sender.send(())?;

            Ok::<_, PlayerError>(0)
        });

        Ok(PlaybackStatus {
            success: true,
            playback_id,
        })
    }

    async fn start_playback(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<(), PlayerError> {
        let mut current_seek = seek;
        let mut retry_count = 0;

        loop {
            if retry_count > 0 {
                sleep(retry_options.unwrap().retry_delay).await;
            }
            let (quality, abort, track_or_id) = {
                let binding = self.active_playback.read().unwrap();
                let playback = binding.as_ref().unwrap();
                (
                    playback.quality,
                    playback.abort.clone(),
                    playback.tracks[playback.position as usize].clone(),
                )
            };
            let track_id = track_or_id.id();
            info!(
                "Playing track with Symphonia: {} {abort:?} {track_or_id:?}",
                track_id
            );
            let playable_track = self
                .track_or_id_to_playable(self.playback_type, &track_or_id, &quality)
                .await?;
            let mss = MediaSourceStream::new(playable_track.source, Default::default());

            #[allow(unused)]
            #[cfg(feature = "cpal")]
            let mut audio_output_handler = AudioOutputHandler::new(Box::new(
                moosicbox_symphonia_player::output::cpal::player::try_open,
            ));
            #[allow(unused)]
            #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
            let mut audio_output_handler = AudioOutputHandler::new(Box::new(
                moosicbox_symphonia_player::output::pulseaudio::simple::try_open,
            ));
            #[allow(unused)]
            #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
            let mut audio_output_handler = AudioOutputHandler::new(Box::new(
                moosicbox_symphonia_player::output::pulseaudio::standard::try_open,
            ));

            let mut handle = PlaybackHandle::new(abort.clone());
            handle.on_progress(move |progress| {
                let mut binding = self.active_playback.write().unwrap();
                if let Some(playback) = binding.as_mut() {
                    let old = playback.clone();
                    playback.progress = progress.secs;
                    trigger_playback_event(playback, &old);
                }
            });

            if let Err(err) = moosicbox_symphonia_player::play_media_source(
                mss,
                &playable_track.hint,
                &mut audio_output_handler,
                true,
                true,
                None,
                current_seek,
                &handle,
            ) {
                if retry_options.is_none() {
                    error!("Track playback failed and no retry options: {err:?}");
                    return Err(PlayerError::PlaybackError(err));
                }

                let retry_options = retry_options.unwrap();

                if let PlaybackError::AudioOutput(AudioOutputError::Interrupt) = err {
                    retry_count += 1;
                    if retry_count > retry_options.max_retry_count {
                        error!("Playback retry failed after {retry_count} attempts. Not retrying");
                        break;
                    }
                    let binding = self.active_playback.read().unwrap();
                    let playback = binding.as_ref().unwrap();
                    current_seek = Some(playback.progress);
                    warn!("Playback interrupted. Trying again at position {current_seek:?} (attempt {retry_count}/{})", retry_options.max_retry_count);
                    continue;
                }
            }
            info!("Finished playback for track {}", track_id);
            break;
        }

        Ok(())
    }

    pub fn stop_track(&self) -> Result<PlaybackStatus, PlayerError> {
        let playback = self.stop()?;

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    pub fn stop(&self) -> Result<Playback, PlayerError> {
        info!("Stopping playback for playback_id");
        let playback = self.get_playback()?;

        if !playback.playing {
            debug!("Playback {playback:?} not playing");
            return Ok(playback);
        }

        debug!("Stopping playback {playback:?}");

        playback.abort.cancel();

        trace!("Waiting for playback completion response");
        if let Err(err) = self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(2))
        {
            error!("Sender correlated with receiver has dropped: {err:?}");
        }
        trace!("Playback successfully stopped");

        Ok(playback)
    }

    pub fn seek_track(
        &self,
        seek: f64,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        let playback = self.stop()?;
        let playback_id = playback.id;
        self.play_playback(playback, Some(seek), retry_options)?;

        Ok(PlaybackStatus {
            success: true,
            playback_id,
        })
    }

    pub fn next_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Playing next track seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position + 1 >= playback.tracks.len() as u16 {
            return Err(PlayerError::PositionOutOfBounds(
                playback.position as i32 + 1,
            ));
        }

        self.update_playback(Some(playback.position + 1), seek, None, retry_options)
    }

    pub fn previous_track(
        &self,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Playing next track seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position == 0 {
            return Err(PlayerError::PositionOutOfBounds(-1));
        }

        self.update_playback(Some(playback.position - 1), seek, None, retry_options)
    }

    pub fn update_playback(
        &self,
        position: Option<u16>,
        seek: Option<f64>,
        tracks: Option<Vec<TrackOrId>>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Updating playback position {position:?} seek {seek:?}");
        let playback = self.get_playback()?;
        let original = playback.clone();

        let playback = Playback {
            id: playback.id,
            session_id: playback.session_id,
            tracks: tracks.unwrap_or_else(|| playback.tracks.clone()),
            playing: playback.playing,
            quality: playback.quality,
            position: position.unwrap_or(playback.position),
            progress: seek.unwrap_or(0.0),
            abort: CancellationToken::new(),
        };

        trigger_playback_event(&playback, &original);

        self.play_playback(playback, seek, retry_options)
    }

    pub fn pause_playback(&self) -> Result<PlaybackStatus, PlayerError> {
        info!("Pausing playback id");
        let playback = self.get_playback()?;

        let id = playback.id;

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        playback.abort.cancel();

        trace!("Waiting for playback completion response");
        if let Err(err) = self.receiver.recv() {
            error!("Sender correlated with receiver has dropped: {err:?}");
        }
        trace!("Playback successfully stopped");

        self.active_playback
            .clone()
            .write()
            .unwrap()
            .replace(Playback {
                id,
                session_id: playback.session_id,
                tracks: playback.tracks.clone(),
                playing: false,
                quality: playback.quality,
                position: playback.position,
                progress: playback.progress,
                abort: CancellationToken::new(),
            });

        Ok(PlaybackStatus {
            success: true,
            playback_id: id,
        })
    }

    pub fn resume_playback(
        &self,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Resuming playback");
        let playback = self.get_playback()?;

        let id = playback.id;

        if playback.playing {
            return Err(PlayerError::PlaybackAlreadyPlaying(id));
        }

        let seek = Some(playback.progress);

        let playback = Playback {
            id,
            session_id: playback.session_id,
            tracks: playback.tracks.clone(),
            playing: true,
            position: playback.position,
            quality: playback.quality,
            progress: playback.progress,
            abort: CancellationToken::new(),
        };

        self.play_playback(playback, seek, retry_options)
    }

    pub fn track_to_playable_file(&self, track: &Track) -> PlayableTrack {
        let mut hint = Hint::new();

        let file = track.file.clone().unwrap();
        let path = Path::new(&file);

        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }

        let source = Box::new(File::open(path).unwrap());

        PlayableTrack {
            track_id: track.id,
            source,
            hint,
        }
    }

    pub async fn track_or_id_to_playable_stream(
        &self,
        track_or_id: &TrackOrId,
        quality: &PlaybackQuality,
    ) -> PlayableTrack {
        match track_or_id {
            TrackOrId::Id(id) => self.track_id_to_playable_stream(*id, quality).await,
            TrackOrId::Track(track) => self.track_to_playable_stream(track, quality).await,
        }
    }

    pub async fn track_to_playable_stream(
        &self,
        track: &Track,
        quality: &PlaybackQuality,
    ) -> PlayableTrack {
        self.track_id_to_playable_stream(track.id, quality).await
    }

    pub async fn track_id_to_playable_stream(
        &self,
        track_id: i32,
        quality: &PlaybackQuality,
    ) -> PlayableTrack {
        let hint = Hint::new();

        match &self.source {
            PlayerSource::Remote {
                host,
                query,
                headers,
            } => {
                let query_string = if let Some(query) = query {
                    let mut serializer = form_urlencoded::Serializer::new(String::new());
                    for (key, value) in query {
                        serializer.append_pair(key, value);
                    }
                    serializer.finish()
                } else {
                    "".to_string()
                };

                let query_string = if query_string.is_empty() {
                    query_string
                } else {
                    format!("{query_string}&")
                };

                let query_string = format!("?{query_string}trackId={track_id}");

                let query_string = match quality.format {
                    AudioFormat::Aac => query_string + "&format=AAC",
                    AudioFormat::Flac => query_string + "&format=FLAC",
                    AudioFormat::Mp3 => query_string + "&format=MP3",
                    AudioFormat::Opus => query_string + "&format=OPUS",
                    AudioFormat::Source => query_string,
                };

                let url = format!("{host}/track/info{query_string}");
                let mut client = reqwest::Client::new().get(&url);

                if let Some(headers) = headers {
                    for (key, value) in headers {
                        client = client.header(key, value);
                    }
                }

                let res: Value = client.send().await.unwrap().json().await.unwrap();
                debug!("Got track info {res:?}");
                let size = res.get("bytes").unwrap().as_u64().unwrap();
                let url = format!("{host}/track{query_string}");

                let source = Box::new(RemoteByteStream::new(url, Some(size), true));

                PlayableTrack {
                    track_id,
                    source,
                    hint,
                }
            }
            PlayerSource::Local => {
                unreachable!();
            }
        }
    }

    async fn track_or_id_to_playable(
        &self,
        playback_type: PlaybackType,
        track_or_id: &TrackOrId,
        quality: &PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        Ok(match playback_type {
            PlaybackType::File => match track_or_id {
                TrackOrId::Id(_id) => return Err(PlayerError::InvalidPlaybackType),
                TrackOrId::Track(track) => self.track_to_playable_file(track),
            },
            PlaybackType::Stream => {
                self.track_or_id_to_playable_stream(track_or_id, quality)
                    .await
            }
            PlaybackType::Default => match track_or_id {
                TrackOrId::Id(id) => self.track_id_to_playable_stream(*id, quality).await,
                TrackOrId::Track(track) => self.track_to_playable_file(track),
            },
        })
    }

    pub fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
        Ok(ApiPlaybackStatus {
            active_playbacks: self
                .active_playback
                .clone()
                .read()
                .unwrap()
                .clone()
                .map(|x| x.to_api()),
        })
    }

    pub fn get_playback(&self) -> Result<Playback, PlayerError> {
        trace!("Getting Playback");
        if let Some(playback) = self.active_playback.read().unwrap().clone() {
            Ok(playback.clone())
        } else {
            Err(PlayerError::NoPlayersPlaying)
        }
    }
}

static PLAYBACK_EVENT_LISTENERS: Lazy<Arc<RwLock<Vec<fn(&UpdateSession, &Playback)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

pub fn on_playback_event(listener: fn(&UpdateSession, &Playback)) {
    PLAYBACK_EVENT_LISTENERS.write().unwrap().push(listener);
}

fn trigger_playback_event(current: &Playback, previous: &Playback) {
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
        Some(current.progress as i32)
    } else {
        None
    };
    let track_ids = current.tracks.iter().map(|t| t.id()).collect::<Vec<_>>();
    let playlist = if track_ids != previous.tracks.iter().map(|t| t.id()).collect::<Vec<_>>() {
        has_change = true;
        Some(moosicbox_core::sqlite::models::UpdateSessionPlaylist {
            session_playlist_id: -1,
            tracks: track_ids,
        })
    } else {
        None
    };

    if !has_change {
        return;
    }

    log::debug!("Triggering playback event: playing={playing:?} position={position:?} seek={seek:?} playlist={playlist:?}");

    let update = UpdateSession {
        session_id: session_id as i32,
        name: None,
        active: None,
        playing,
        position,
        seek,
        playlist,
    };

    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(&update, current);
    }
}
