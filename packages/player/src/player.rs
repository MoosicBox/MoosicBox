use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, RwLock},
    u16, usize,
};

use atomic_float::AtomicF64;
use crossbeam_channel::{bounded, Receiver, SendError};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album_tracks, get_track},
        models::{ToApi, Track, UpdateSession},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStream,
    output::{AudioOutputError, AudioOutputHandler},
    volume_mixer::mix_volume,
    PlaybackError,
};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
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
    ) -> Playback {
        Playback {
            id: thread_rng().gen::<usize>(),
            session_id,
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
    Track(Box<Track>),
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
    pub fn play_album(
        &self,
        db: Db,
        session_id: Option<usize>,
        album_id: i32,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
        quality: PlaybackQuality,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        let tracks = {
            let library = db.library.lock().unwrap();
            get_album_tracks(&library.inner, album_id).map_err(|e| {
                error!("Failed to fetch album tracks: {e:?}");
                PlayerError::AlbumFetchFailed(album_id)
            })?
        };

        self.play_tracks(
            Some(db),
            session_id,
            tracks
                .into_iter()
                .map(Box::new)
                .map(TrackOrId::Track)
                .collect(),
            position,
            seek,
            volume,
            quality,
            retry_options,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play_track(
        &self,
        db: Option<Db>,
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
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play_tracks(
        &self,
        db: Option<Db>,
        session_id: Option<usize>,
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        seek: Option<f64>,
        volume: Option<f64>,
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
                        let track = get_track(&library.inner, *track_id)
                            .map_err(|e| {
                                error!("Failed to fetch track: {e:?}");
                                PlayerError::TrackFetchFailed(*track_id)
                            })
                            .expect("Failed to fetch track");
                        debug!("Got track {track:?}");
                        TrackOrId::Track(Box::new(track.expect("Track doesn't exist")))
                    }
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
        );

        self.play_playback(playback, seek, retry_options)
    }

    pub fn play_playback(
        &self,
        mut playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Playing playback...");
        if let Ok(playback) = self.get_playback() {
            debug!("Stopping existing playback {}", playback.id);
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

        playback.playing = true;

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
            let abort = playback.abort.clone();

            while !abort.is_cancelled()
                && playback.playing
                && (playback.position as usize) < playback.tracks.len()
            {
                let track_or_id = &playback.tracks[playback.position as usize];
                debug!("track {track_or_id:?} {seek:?}");

                let seek = if seek.is_some() { seek.take() } else { None };

                if let Err(err) = player.start_playback(seek, retry_options).await {
                    log::error!("Playback error occurred: {err:?}");
                    player
                        .active_playback
                        .write()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .playing = false;
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

            player
                .active_playback
                .write()
                .unwrap()
                .as_mut()
                .unwrap()
                .playing = false;

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
            info!(
                "Playing track with Symphonia: {} {abort:?} {track_or_id:?}",
                track_id
            );
            let playable_track = self
                .track_or_id_to_playable(self.playback_type, &track_or_id, &quality)
                .await?;
            let mss = MediaSourceStream::new(playable_track.source, Default::default());

            let mut audio_output_handler = AudioOutputHandler::new();

            #[allow(unused)]
            #[cfg(feature = "cpal")]
            audio_output_handler.with_output(Box::new(
                moosicbox_symphonia_player::output::cpal::player::try_open,
            ));
            #[allow(unused)]
            #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
            audio_output_handler.with_output(Box::new(
                moosicbox_symphonia_player::output::pulseaudio::simple::try_open,
            ));
            #[allow(unused)]
            #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
            audio_output_handler.with_output(Box::new(
                moosicbox_symphonia_player::output::pulseaudio::standard::try_open,
            ));

            let active_playback = self.active_playback.clone();

            audio_output_handler
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

        debug!("Aborting playback {playback:?} for stop");
        playback.abort.cancel();

        if !playback.playing {
            debug!("Playback not playing: {playback:?}");
            return Ok(playback);
        }

        trace!("Waiting for playback completion response");
        if let Some(receiver) = self.receiver.write().unwrap().take() {
            if let Err(err) = receiver.recv_timeout(std::time::Duration::from_secs(5)) {
                match err {
                    crossbeam_channel::RecvTimeoutError::Timeout => {
                        log::error!("Playback timed out waiting for abort completion")
                    }
                    crossbeam_channel::RecvTimeoutError::Disconnected => {
                        log::error!("Sender associated with playback disconnected")
                    }
                }
            } else {
                trace!("Playback successfully stopped");
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
            retry_options,
        )
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
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::debug!(
            "\
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
            "
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
            )
        };

        log::debug!("update_playback: should_play={should_play}");

        let original = playback.clone();

        let playback = Playback {
            id: playback.id,
            session_id: playback.session_id,
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
        info!("Pausing playback id");
        let playback = self.get_playback()?;

        let id = playback.id;

        info!("Aborting playback id {id} for pause");
        playback.abort.cancel();

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        trace!("Waiting for playback completion response");
        if let Some(receiver) = self.receiver.write().unwrap().take() {
            if let Err(err) = receiver.recv() {
                error!("Sender correlated with receiver has dropped: {err:?}");
            }
        } else {
            log::debug!("No receiver to wait for completion response with");
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
                volume: playback.volume,
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
            volume: playback.volume,
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

                let url = format!("{host}/track{query_string}");
                let mut client = reqwest::Client::new().head(&url);

                if let Some(headers) = headers {
                    for (key, value) in headers {
                        client = client.header(key, value);
                    }
                }

                let res = client.send().await.unwrap();
                let size = res
                    .headers()
                    .get("content-length")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                let url = format!("{host}/track{query_string}");

                let source = Box::new(RemoteByteStream::new(
                    url,
                    Some(size),
                    true,
                    self.active_playback
                        .read()
                        .unwrap()
                        .as_ref()
                        .map(|p| p.abort.clone())
                        .unwrap_or_default(),
                ));

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
