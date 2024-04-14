use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, RwLock},
    u16, usize,
};

use atomic_float::AtomicF64;
use crossbeam_channel::{bounded, Receiver, SendError};
use futures::{StreamExt as _, TryStreamExt as _};
use lazy_static::lazy_static;
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
use moosicbox_json_utils::{serde_json::ToValue, ParseError};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use moosicbox_symphonia_player::{
    media_sources::{bytestream_source::ByteStreamSource, remote_bytestream::RemoteByteStream},
    output::{AudioOutputError, AudioOutputHandler},
    signal_chain::{SignalChain, SignalChainError},
    volume_mixer::mix_volume,
    PlaybackError,
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
use tokio_util::{
    codec::{BytesCodec, FramedRead},
    sync::CancellationToken,
};
use url::form_urlencoded;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

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
    db: &Box<dyn Database>,
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

#[derive(Clone)]
pub struct Player {
    pub id: usize,
    playback_type: PlaybackType,
    source: PlayerSource,
    pub active_playback: Arc<RwLock<Option<Playback>>>,
    receiver: Arc<RwLock<Option<Receiver<()>>>>,
}

impl Player {
    pub fn new(source: PlayerSource, playback_type: Option<PlaybackType>) -> Player {
        Player {
            id: thread_rng().gen::<usize>(),
            playback_type: playback_type.unwrap_or_default(),
            source,
            active_playback: Arc::new(RwLock::new(None)),
            receiver: Arc::new(RwLock::new(None)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn play_album(
        &self,
        db: &Box<dyn Database>,
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
                .map(|t| Track::Library(t))
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
    pub async fn play_track(
        &self,
        db: &Box<dyn Database>,
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
    pub async fn play_tracks(
        &self,
        db: &Box<dyn Database>,
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

    pub fn play_playback(
        &self,
        mut playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Playing playback...");
        if let Ok(playback) = self.get_playback() {
            log::debug!("Stopping existing playback {}", playback.id);
            self.stop()?;
        }

        if playback.tracks.is_empty() {
            log::debug!("No tracks to play for {playback:?}");
            return Ok(PlaybackStatus {
                success: true,
                playback_id: playback.id,
            });
        }

        let (tx, rx) = bounded(1);

        self.receiver.write().unwrap().replace(rx);

        let old = playback.clone();

        playback.playing = true;

        trigger_playback_event(&playback, &old);

        self.active_playback
            .write()
            .unwrap()
            .replace(playback.clone());

        let player = self.clone();

        log::debug!(
            "Playing playback: position={} tracks={:?}",
            playback.position,
            playback
                .tracks
                .iter()
                .map(|t| t.to_id())
                .collect::<Vec<_>>()
        );
        let playback_id = playback.id;

        RT.spawn(async move {
            let mut seek = seek;
            let mut playback = playback.clone();
            let abort = playback.abort.clone();

            while !abort.is_cancelled()
                && playback.playing
                && (playback.position as usize) < playback.tracks.len()
            {
                let track_or_id = &playback.tracks[playback.position as usize];
                log::debug!("track {track_or_id:?} {seek:?}");

                let seek = if seek.is_some() { seek.take() } else { None };

                if let Err(err) = player.start_playback(seek, retry_options).await {
                    log::error!("Playback error occurred: {err:?}");

                    let mut binding = player.active_playback.write().unwrap();
                    let active = binding.as_mut().unwrap();
                    let old = active.clone();
                    active.playing = false;
                    trigger_playback_event(active, &old);

                    tx.send(())?;
                    return Err(err);
                }

                if abort.is_cancelled() {
                    log::debug!("Playback cancelled. Breaking");
                    break;
                }

                let mut binding = player.active_playback.write().unwrap();
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
                "Finished playback on all tracks. aborted={} playing={} position={} len={}",
                abort.is_cancelled(),
                playback.playing,
                playback.position,
                playback.tracks.len()
            );

            let mut binding = player.active_playback.write().unwrap();
            let active = binding.as_mut().unwrap();
            let old = active.clone();
            active.playing = false;

            if !abort.is_cancelled() {
                trigger_playback_event(active, &old);
            }

            tx.send(())?;

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
        log::debug!("start_playback: seek={seek:?}");
        let mut current_seek = seek;
        let mut retry_count = 0;
        let abort = self
            .active_playback
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .abort
            .clone();

        while !abort.is_cancelled() {
            if retry_count > 0 {
                sleep(retry_options.unwrap().retry_delay).await;
            }
            let (quality, volume, abort, track_or_id) = {
                let binding = self.active_playback.read().unwrap();
                let playback = binding.as_ref().unwrap();
                (
                    playback.quality,
                    playback.volume.clone(),
                    playback.abort.clone(),
                    playback.tracks[playback.position as usize].clone(),
                )
            };
            let track_id = track_or_id.id();
            log::info!(
                "Playing track with Symphonia: {} {abort:?} {track_or_id:?}",
                track_id
            );

            let playback_type = match track_or_id.track_source() {
                TrackApiSource::Local => self.playback_type,
                _ => PlaybackType::Stream,
            };

            let playable_track = self
                .track_or_id_to_playable(playback_type, &track_or_id, &quality)
                .await?;
            let mss = MediaSourceStream::new(playable_track.source, Default::default());

            let active_playback = self.active_playback.clone();

            let mut audio_output_handler = AudioOutputHandler::new()
                .with_filter(Box::new(move |_decoded, packet, track| {
                    if let Some(tb) = track.codec_params.time_base {
                        let ts = packet.ts();
                        let t = tb.calc_time(ts);
                        let secs = f64::from(t.seconds as u32) + t.frac;

                        let mut binding = active_playback.write().unwrap();
                        if let Some(playback) = binding.as_mut() {
                            let old = playback.clone();
                            playback.progress = secs;
                            trigger_playback_event(playback, &old);
                        }
                    }
                    Ok(())
                }))
                .with_filter(Box::new(move |decoded, _packet, _track| {
                    mix_volume(decoded, volume.load(std::sync::atomic::Ordering::SeqCst));
                    Ok(())
                }))
                .with_cancellation_token(abort.clone());

            #[cfg(feature = "cpal")]
            {
                audio_output_handler = audio_output_handler.with_output(Box::new(
                    moosicbox_symphonia_player::output::cpal::player::try_open,
                ));
            }
            #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
            {
                audio_output_handler = audio_output_handler.with_output(Box::new(
                    moosicbox_symphonia_player::output::pulseaudio::simple::try_open,
                ));
            }
            #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
            {
                audio_output_handler = audio_output_handler.with_output(Box::new(
                    moosicbox_symphonia_player::output::pulseaudio::standard::try_open,
                ));
            }

            if !audio_output_handler.contains_outputs_to_open() {
                log::warn!("No outputs set for the audio_output_handler");
            }

            if let Err(err) = moosicbox_symphonia_player::play_media_source(
                mss,
                &playable_track.hint,
                &mut audio_output_handler,
                true,
                true,
                None,
                current_seek,
            ) {
                if retry_options.is_none() {
                    log::error!("Track playback failed and no retry options: {err:?}");
                    return Err(PlayerError::PlaybackError(err));
                }

                let retry_options = retry_options.unwrap();

                if let PlaybackError::AudioOutput(AudioOutputError::Interrupt) = err {
                    retry_count += 1;
                    if retry_count > retry_options.max_retry_count {
                        log::error!(
                            "Playback retry failed after {retry_count} attempts. Not retrying"
                        );
                        break;
                    }
                    let binding = self.active_playback.read().unwrap();
                    let playback = binding.as_ref().unwrap();
                    current_seek = Some(playback.progress);
                    log::warn!("Playback interrupted. Trying again at position {current_seek:?} (attempt {retry_count}/{})", retry_options.max_retry_count);
                    continue;
                }
            }
            log::info!("Finished playback for track {}", track_id);
            break;
        }

        Ok(())
    }

    pub fn stop_track(&self) -> Result<PlaybackStatus, PlayerError> {
        log::debug!("stop_track called");
        let playback = self.stop()?;

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    pub fn stop(&self) -> Result<Playback, PlayerError> {
        log::info!("Stopping playback");
        let playback = self.get_playback()?;

        log::debug!("Aborting playback {playback:?} for stop");
        playback.abort.cancel();

        if !playback.playing {
            log::debug!("Playback not playing: {playback:?}");
            return Ok(playback);
        }

        log::trace!("Waiting for playback completion response");
        if let Some(receiver) = self.receiver.write().unwrap().take() {
            if let Err(err) = receiver.recv_timeout(std::time::Duration::from_secs(5)) {
                match err {
                    crossbeam_channel::RecvTimeoutError::Timeout => {
                        log::error!("Playback timed out waiting for abort completion")
                    }
                    crossbeam_channel::RecvTimeoutError::Disconnected => {
                        log::info!("Sender associated with playback disconnected")
                    }
                }
            } else {
                log::trace!("Playback successfully stopped");
            }
        } else {
            log::debug!("No receiver to wait for completion response with");
        }

        Ok(playback)
    }

    pub fn seek_track(
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

    pub fn next_track(
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
    }

    pub fn previous_track(
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
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_playback(
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
    ) -> Result<PlaybackStatus, PlayerError> {
        log::debug!(
            "\
            source={:?}\n\t\
            update_playback:\n\t\
            play={play:?}\n\t\
            stop={stop:?}\n\t\
            playing={playing:?}\n\t\
            position={position:?}\n\t\
            seek={seek:?}\n\t\
            volume={volume:?}\n\t\
            tracks={tracks:?}\n\t\
            quality={quality:?}\n\t\
            session_id={session_id:?}\
            ",
            self.source
        );

        if stop.unwrap_or(false) {
            return Ok(PlaybackStatus {
                success: true,
                playback_id: self.stop()?.id,
            });
        }

        let mut should_play = play.unwrap_or(false);

        let playback = if let Ok(playback) = self.get_playback() {
            log::trace!("update_playback: existing playback={playback:?}");
            if playback.playing {
                if let Some(false) = playing {
                    self.stop()?;
                }
            } else {
                should_play = should_play || playing.unwrap_or(false);
            }

            playback
        } else {
            log::trace!("update_playback: no existing playback");
            should_play = should_play || playing.unwrap_or(false);

            Playback::new(
                tracks.clone().unwrap_or_default(),
                position,
                AtomicF64::new(volume.unwrap_or(1.0)),
                quality.unwrap_or_default(),
                session_id,
                session_playlist_id,
            )
        };

        log::debug!("update_playback: should_play={should_play}");

        let original = playback.clone();

        let playback = Playback {
            id: playback.id,
            session_id: playback.session_id,
            session_playlist_id: playback.session_playlist_id,
            tracks: tracks.unwrap_or_else(|| playback.tracks.clone()),
            playing: playing.unwrap_or(playback.playing),
            quality: quality.unwrap_or(playback.quality),
            position: position.unwrap_or(playback.position),
            progress: if play.unwrap_or(false) {
                seek.unwrap_or(0.0)
            } else {
                seek.unwrap_or(playback.progress)
            },
            volume: playback.volume,
            abort: if should_play {
                CancellationToken::new()
            } else {
                playback.abort
            },
        };

        if let Some(volume) = volume {
            playback
                .volume
                .store(volume, std::sync::atomic::Ordering::SeqCst);
        }

        trigger_playback_event(&playback, &original);

        let playback_id = playback.id;
        let seek = if playback.progress != 0.0 {
            Some(playback.progress)
        } else {
            None
        };

        if should_play {
            self.play_playback(playback, seek, retry_options)
        } else {
            log::debug!("update_playback: updating active playback to {playback:?}");
            self.active_playback
                .write()
                .unwrap()
                .replace(playback.clone());

            Ok(PlaybackStatus {
                success: true,
                playback_id,
            })
        }
    }

    pub fn pause_playback(&self) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Pausing playback id");
        let mut playback = self.get_playback()?;

        let id = playback.id;

        log::info!("Aborting playback id {id} for pause");
        playback.abort.cancel();

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        log::trace!("Waiting for playback completion response");
        if let Some(receiver) = self.receiver.write().unwrap().take() {
            if let Err(err) = receiver.recv() {
                log::error!("Sender correlated with receiver has dropped: {err:?}");
            }
        } else {
            log::debug!("No receiver to wait for completion response with");
        }
        log::trace!("Playback successfully stopped");

        playback.playing = false;
        playback.abort = CancellationToken::new();

        self.active_playback
            .clone()
            .write()
            .unwrap()
            .replace(playback);

        Ok(PlaybackStatus {
            success: true,
            playback_id: id,
        })
    }

    pub fn resume_playback(
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

    pub async fn track_to_playable_file(
        &self,
        track: &LibraryTrack,
        quality: &PlaybackQuality,
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
                AudioFormat::Flac => return Err(PlayerError::UnsupportedFormat(quality.format)),
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
                _ | AudioFormat::Source => {}
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

    pub async fn track_or_id_to_playable_stream(
        &self,
        track_or_id: &TrackOrId,
        quality: &PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        match track_or_id {
            TrackOrId::Id(id, source) => {
                self.track_id_to_playable_stream(*id, *source, quality)
                    .await
            }
            TrackOrId::Track(track) => self.track_to_playable_stream(track, quality).await,
        }
    }

    pub async fn track_to_playable_stream(
        &self,
        track: &Track,
        quality: &PlaybackQuality,
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

    pub async fn track_id_to_playable_stream(
        &self,
        track_id: i32,
        source: ApiSource,
        quality: &PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        let (host, query, headers) = match &self.source {
            PlayerSource::Remote {
                host,
                query,
                headers,
            } => (host.to_string(), query, headers),
            PlayerSource::Local => (
                format!(
                    "http://127.0.0.1:{}",
                    SERVICE_PORT
                        .read()
                        .unwrap()
                        .expect("Missing SERVICE_PORT value")
                ),
                &None,
                &None,
            ),
        };

        let query_params = {
            let mut serializer = form_urlencoded::Serializer::new(String::new());

            if let Some(query) = query {
                for (key, value) in query {
                    serializer.append_pair(key, value);
                }
            }

            serializer.append_pair("trackId", &track_id.to_string());

            match source {
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

        let url = match source {
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
                    .ok_or(PlayerError::TrackFetchFailed(track_id))
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

        let source = Box::new(RemoteByteStream::new(
            url,
            size,
            true,
            #[cfg(feature = "flac")]
            {
                quality.format == AudioFormat::Flac
            },
            #[cfg(not(feature = "flac"))]
            false,
            self.active_playback
                .read()
                .unwrap()
                .as_ref()
                .map(|p| p.abort.clone())
                .unwrap_or_default(),
        ));

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
            track_id,
            source,
            hint,
        })
    }

    async fn track_or_id_to_playable(
        &self,
        playback_type: PlaybackType,
        track_or_id: &TrackOrId,
        quality: &PlaybackQuality,
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
        log::trace!("Getting Playback");
        if let Some(playback) = self.active_playback.read().unwrap().clone() {
            Ok(playback.clone())
        } else {
            Err(PlayerError::NoPlayersPlaying)
        }
    }
}

static SERVICE_PORT: Lazy<RwLock<Option<u16>>> = Lazy::new(|| RwLock::new(None));

pub fn set_service_port(service_port: u16) {
    SERVICE_PORT.write().unwrap().replace(service_port);
}

type PlaybackEventCallback = fn(&UpdateSession, &Playback);

static PLAYBACK_EVENT_LISTENERS: Lazy<Arc<RwLock<Vec<PlaybackEventCallback>>>> =
    Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

pub fn on_playback_event(listener: PlaybackEventCallback) {
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

    for listener in PLAYBACK_EVENT_LISTENERS.read().unwrap().iter() {
        listener(&update, current);
    }
}
