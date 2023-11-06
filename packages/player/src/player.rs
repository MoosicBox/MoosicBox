use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    u16, usize,
};

use crossbeam_channel::{bounded, Receiver, SendError, Sender};
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album_tracks, get_track},
        models::{ToApi, Track},
    },
};
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStream, AudioOutputType, PlaybackError, Progress,
};
use rand::{thread_rng, Rng as _};
use serde::Serialize;
use symphonia::core::{
    io::{MediaSource, MediaSourceStream},
    probe::Hint,
};
use thiserror::Error;
use tokio::runtime::{self, Runtime};

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

impl From<SendError<()>> for PlayerError {
    fn from(err: SendError<()>) -> Self {
        PlayerError::Send(err)
    }
}

impl From<PlaybackError> for PlayerError {
    fn from(err: PlaybackError) -> Self {
        PlayerError::PlaybackError(err)
    }
}

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(SendError<()>),
    #[error(transparent)]
    PlaybackError(moosicbox_symphonia_player::PlaybackError),
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
    pub tracks: Vec<TrackOrId>,
    pub playing: bool,
    pub position: u16,
    pub progress: Arc<RwLock<Progress>>,
    pub abort: Arc<AtomicBool>,
}

impl Playback {
    pub fn new(tracks: Vec<TrackOrId>, position: Option<u16>) -> Playback {
        Playback {
            id: thread_rng().gen::<usize>(),
            tracks,
            playing: true,
            position: position.unwrap_or_default(),
            progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
            abort: Arc::new(AtomicBool::new(false)),
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
            seek: self.progress.clone().read().unwrap().position,
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

pub struct PlayableTrack {
    pub track_id: i32,
    pub source: Box<dyn MediaSource>,
    pub hint: Hint,
}

#[derive(Copy, Clone)]
pub enum PlaybackType {
    File,
    Stream,
    Default,
}

#[derive(Clone)]
pub struct Player {
    pub id: usize,
    host: Option<String>,
    pub active_playback: Arc<RwLock<Option<Playback>>>,
    sender: Sender<()>,
    receiver: Receiver<()>,
}

impl Player {
    pub fn new(host: Option<String>) -> Player {
        let (tx, rx) = bounded(1);

        Player {
            id: thread_rng().gen::<usize>(),
            host,
            active_playback: Arc::new(RwLock::new(None)),
            sender: tx,
            receiver: rx,
        }
    }

    pub fn play_album(
        &self,
        db: Db,
        album_id: i32,
        position: Option<u16>,
        seek: Option<f64>,
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
            tracks.into_iter().map(TrackOrId::Track).collect(),
            position,
            seek,
        )
    }

    pub fn play_track(
        &self,
        db: Option<Db>,
        track: TrackOrId,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        self.play_tracks(db, vec![track], None, seek)
    }

    pub fn play_tracks(
        &self,
        db: Option<Db>,
        tracks: Vec<TrackOrId>,
        position: Option<u16>,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Ok(playback) = self.get_playback() {
            debug!("Stopping existing playback {}", playback.id);
            self.stop(Some(playback.id))?;
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
                        TrackOrId::Track(track.expect("Track doesnt exist"))
                    }
                    TrackOrId::Track(track) => TrackOrId::Track(track.clone()),
                })
                .collect()
        };
        let playback = Playback::new(tracks, position);

        self.play_playback(playback, PlaybackType::Default, seek)
    }

    fn assert_playback_playing(&self, playback_id: usize) -> Result<(), PlayerError> {
        if self
            .active_playback
            .read()
            .unwrap()
            .clone()
            .is_some_and(|p| p.id != playback_id)
        {
            return Err(PlayerError::PlaybackNotPlaying(playback_id));
        }

        Ok(())
    }

    pub fn play_playback(
        &self,
        playback: Playback,
        to_playable: PlaybackType,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        self.assert_playback_playing(playback.id)?;
        if let Ok(playback) = self.get_playback() {
            debug!("Stopping existing playback {}", playback.id);
            self.stop(Some(playback.id))?;
        }

        self.active_playback
            .write()
            .unwrap()
            .replace(playback.clone());

        let player = self.clone();

        RT.spawn(async move {
            let mut seek = seek;

            for (i, track_or_id) in playback.tracks.iter().enumerate() {
                if (i as u16) < playback.position {
                    continue;
                }

                let playback = Playback {
                    id: playback.id,
                    tracks: playback.tracks.clone(),
                    playing: true,
                    position: i as u16,
                    progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
                    abort: Arc::new(AtomicBool::new(false)),
                };

                player
                    .active_playback
                    .write()
                    .unwrap()
                    .replace(playback.clone());

                debug!("track {track_or_id:?} {seek:?}");

                let seek = if seek.is_some() { seek.take() } else { None };

                player.start_playback(&playback, to_playable, seek)?;

                if playback.abort.load(Ordering::SeqCst) {
                    break;
                }
            }

            player.active_playback.write().unwrap().take();
            player.sender.send(())?;

            Ok::<_, PlayerError>(0)
        });

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    fn start_playback(
        &self,
        playback: &Playback,
        playback_type: PlaybackType,
        seek: Option<f64>,
    ) -> Result<(), PlayerError> {
        let track_or_id = &playback.tracks[playback.position as usize];
        let track_id = match track_or_id {
            TrackOrId::Id(id) => *id,
            TrackOrId::Track(track_or_id) => track_or_id.id,
        };
        info!("Playing track with Symphonia: {}", track_id);

        #[allow(unused)]
        #[cfg(feature = "cpal")]
        let audio_output_type = AudioOutputType::Cpal;
        #[allow(unused)]
        #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
        let audio_output_type = AudioOutputType::PulseAudioSimple;
        #[allow(unused)]
        #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
        let audio_output_type = AudioOutputType::PulseAudioStandard;

        let playable_track = self.track_or_id_to_playable(playback_type, track_or_id)?;
        let mss = MediaSourceStream::new(playable_track.source, Default::default());
        moosicbox_symphonia_player::play_media_source(
            mss,
            &playable_track.hint,
            &audio_output_type,
            true,
            true,
            None,
            seek,
            playback.progress.clone(),
            playback.abort.clone(),
        )?;

        info!("Finished playback for track {}", track_id);

        Ok(())
    }

    pub fn stop_track(&self, playback_id: Option<usize>) -> Result<PlaybackStatus, PlayerError> {
        let playback = self.stop(playback_id)?;

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    pub fn stop(&self, playback_id: Option<usize>) -> Result<Playback, PlayerError> {
        if let Some(playback_id) = playback_id {
            self.assert_playback_playing(playback_id)?;
        }
        info!("Stopping playback for playback_id {playback_id:?}");
        let playback = self.get_playback()?;

        if !playback.playing {
            debug!("Playback {playback:?} not playing");
            return Ok(playback);
        }

        debug!("Stopping playback {playback:?}");

        playback.abort.clone().store(true, Ordering::SeqCst);

        trace!("Waiting for playback completion response");
        if let Err(_err) = self.receiver.recv() {
            error!("Sender correlated with receiver has dropped");
        }
        trace!("Playback successfully stopped");

        Ok(playback)
    }

    pub fn seek_track(
        &self,
        playback_id: Option<usize>,
        seek: f64,
    ) -> Result<PlaybackStatus, PlayerError> {
        let playback = self.stop(playback_id)?;
        self.start_playback(&playback, PlaybackType::Default, Some(seek))?;

        Ok(PlaybackStatus {
            success: true,
            playback_id: playback.id,
        })
    }

    pub fn next_track(
        &self,
        playback_id: Option<usize>,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Some(playback_id) = playback_id {
            self.assert_playback_playing(playback_id)?;
        }
        info!("Playing next track {playback_id:?} seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position + 1 >= playback.tracks.len() as u16 {
            return Err(PlayerError::PositionOutOfBounds(
                playback.position as i32 + 1,
            ));
        }

        self.update_playback(playback_id, Some(playback.position + 1), seek)
    }

    pub fn previous_track(
        &self,
        playback_id: Option<usize>,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Some(playback_id) = playback_id {
            self.assert_playback_playing(playback_id)?;
        }
        info!("Playing next track {playback_id:?} seek {seek:?}");
        let playback = self.get_playback()?;

        if playback.position == 0 {
            return Err(PlayerError::PositionOutOfBounds(-1));
        }

        self.update_playback(playback_id, Some(playback.position - 1), seek)
    }

    pub fn update_playback(
        &self,
        playback_id: Option<usize>,
        position: Option<u16>,
        seek: Option<f64>,
    ) -> Result<PlaybackStatus, PlayerError> {
        info!("Updating playback id {playback_id:?} position {position:?} seek {seek:?}");
        let playback = self.stop(playback_id)?;

        let playback = Playback {
            id: playback.id,
            tracks: playback.tracks.clone(),
            playing: true,
            position: position.unwrap_or(playback.position),
            progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
            abort: Arc::new(AtomicBool::new(false)),
        };

        self.play_playback(playback, PlaybackType::Default, seek)
    }

    pub fn pause_playback(
        &self,
        playback_id: Option<usize>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Some(playback_id) = playback_id {
            self.assert_playback_playing(playback_id)?;
        }
        info!("Pausing playback id {playback_id:?}");
        let playback = self.get_playback()?;

        let id = playback.id;

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        playback.abort.clone().store(true, Ordering::SeqCst);

        trace!("Waiting for playback completion response");
        if let Err(_err) = self.receiver.recv() {
            error!("Sender correlated with receiver has dropped");
        }
        trace!("Playback successfully stopped");

        self.active_playback
            .clone()
            .write()
            .unwrap()
            .replace(Playback {
                id,
                tracks: playback.tracks.clone(),
                playing: false,
                position: playback.position,
                progress: playback.progress,
                abort: Arc::new(AtomicBool::new(false)),
            });

        Ok(PlaybackStatus {
            success: true,
            playback_id: id,
        })
    }

    pub fn resume_playback(
        &self,
        playback_id: Option<usize>,
    ) -> Result<PlaybackStatus, PlayerError> {
        if let Some(playback_id) = playback_id {
            self.assert_playback_playing(playback_id)?;
        }
        info!("Resuming playback");
        let playback = self.get_playback()?;

        let id = playback.id;

        if playback.playing {
            return Err(PlayerError::PlaybackAlreadyPlaying(id));
        }

        let seek = Some(playback.progress.read().unwrap().position);

        let playback = Playback {
            id,
            tracks: playback.tracks.clone(),
            playing: true,
            position: playback.position,
            progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
            abort: Arc::new(AtomicBool::new(false)),
        };

        self.play_playback(playback, PlaybackType::Default, seek)
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

    pub fn track_or_id_to_playable_stream(
        &self,
        track_or_id: &TrackOrId,
        host: &str,
    ) -> PlayableTrack {
        match track_or_id {
            TrackOrId::Id(id) => self.track_id_to_playable_stream(*id, host),
            TrackOrId::Track(track) => self.track_to_playable_stream(track, host),
        }
    }

    pub fn track_to_playable_stream(&self, track: &Track, host: &str) -> PlayableTrack {
        self.track_id_to_playable_stream(track.id, host)
    }

    pub fn track_id_to_playable_stream(&self, track_id: i32, host: &str) -> PlayableTrack {
        let hint = Hint::new();

        let source = Box::new(RemoteByteStream::new(
            format!("{host}/track?trackId={}", track_id),
            None,
        ));

        PlayableTrack {
            track_id,
            source,
            hint,
        }
    }

    pub fn track_or_id_to_playable(
        &self,
        playback_type: PlaybackType,
        track_or_id: &TrackOrId,
    ) -> Result<PlayableTrack, PlayerError> {
        Ok(match playback_type {
            PlaybackType::File => match track_or_id {
                TrackOrId::Id(_id) => return Err(PlayerError::InvalidPlaybackType),
                TrackOrId::Track(track) => self.track_to_playable_file(track),
            },
            PlaybackType::Stream => self.track_or_id_to_playable_stream(
                track_or_id,
                self.host.as_ref().expect("Player url value missing"),
            ),
            PlaybackType::Default => match track_or_id {
                TrackOrId::Id(id) => self.track_id_to_playable_stream(
                    *id,
                    self.host.as_ref().expect("Player url value missing"),
                ),
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
