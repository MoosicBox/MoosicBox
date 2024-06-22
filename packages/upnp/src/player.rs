use std::{
    sync::{atomic::AtomicBool, Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use atomic_float::AtomicF64;
use crossbeam_channel::{bounded, Receiver};
use flume::{unbounded, Sender};
use moosicbox_core::{
    sqlite::{
        db::get_track,
        models::{ApiSource, ToApi, UpdateSession},
    },
    types::PlaybackQuality,
};
use moosicbox_database::Database;
use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use moosicbox_symphonia_player::media_sources::remote_bytestream::RemoteByteStreamMediaSource;
use rand::{thread_rng, Rng as _};
use rupnp::{Device, Service};
use symphonia::core::probe::Hint;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use moosicbox_player::player::{
    get_track_url, send_playback_event, trigger_playback_event, ApiPlaybackStatus, PlayableTrack,
    Playback, PlaybackRetryOptions, PlaybackStatus, Player, PlayerError, PlayerSource, TrackOrId,
    RT,
};

use crate::listener::Handle;

pub const DEFAULT_SEEK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_retry_count: 10,
    retry_delay: std::time::Duration::from_millis(100),
};

#[derive(Clone)]
pub struct UpnpPlayer {
    pub db: Arc<Box<dyn Database>>,
    pub id: usize,
    source: PlayerSource,
    transport_uri: Arc<tokio::sync::RwLock<Option<String>>>,
    pub active_playback: Arc<RwLock<Option<Playback>>>,
    play_lock: Arc<tokio::sync::Semaphore>,
    receiver: Arc<RwLock<Option<Receiver<()>>>>,
    handle: Handle,
    device: Device,
    service: Service,
    instance_id: u32,
    media_info_subscription_id: Arc<tokio::sync::RwLock<usize>>,
    position_info_subscription_id: Arc<tokio::sync::RwLock<usize>>,
    transport_info_subscription_id: Arc<tokio::sync::RwLock<usize>>,
    expected_state: Arc<RwLock<Option<String>>>,
}

#[async_trait]
impl Player for UpnpPlayer {
    async fn play_playback(
        &self,
        mut playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Playing playback...");

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
        let current_seek = Arc::new(RwLock::new(seek));
        let mut retry_count = 0;
        let abort = self.get_playback().unwrap().abort.clone();

        while !abort.is_cancelled() {
            log::debug!("Beginning a new playback, locking play_lock");
            let (start_tx, start_rx) = unbounded();
            let semaphore = self.play_lock.clone();
            tokio::spawn(async move {
                let permit = semaphore.acquire().await?;
                if let Err(e) = start_rx.recv_async().await {
                    log::error!("Failed to recv: {e:?}");
                }
                log::debug!("Playback has started, releasing play_lock");
                drop(permit);
                Ok::<_, tokio::sync::AcquireError>(())
            });
            self.unsubscribe().await?;

            if retry_count > 0 {
                sleep(retry_options.unwrap().retry_delay).await;
            }
            let (quality, _volume, abort, track_or_id) = {
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
                "Playing track with UPnP: {} {abort:?} {track_or_id:?}",
                track_id
            );

            let (finished_tx, finished_rx) = unbounded();

            let sent_playback_start_event = Arc::new(AtomicBool::new(false));

            let (transport_uri, headers) = get_track_url(
                track_id.try_into().unwrap(),
                track_or_id.api_source(),
                &self.source,
                quality,
                true,
            )
            .await?;
            self.transport_uri
                .write()
                .await
                .replace(transport_uri.clone());
            log::debug!("start_playback: Set transport_uri={transport_uri}");
            let format = "flac";

            let mut client = reqwest::Client::new().head(&transport_uri);

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
            let track = get_track(&**self.db, track_or_id.id().try_into().unwrap()).await?;
            let duration = track.as_ref().map(|x| x.duration.ceil() as u32);
            let title = track.as_ref().map(|x| x.title.to_owned());
            let artist = track.as_ref().map(|x| x.artist.to_owned());
            let album = track.as_ref().map(|x| x.album.to_owned());
            let track_number = track.map(|x| x.number as u32);

            crate::set_av_transport_uri(
                &self.service,
                self.device.url(),
                self.instance_id,
                &transport_uri,
                format,
                title.as_deref(),
                artist.as_deref(),
                artist.as_deref(),
                album.as_deref(),
                track_number,
                duration,
                size,
            )
            .await
            .map_err(|e| {
                log::error!("set_av_transport_uri failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

            let current_seek = current_seek.clone();
            let seek = { current_seek.read().unwrap().as_ref().cloned() };

            if let Some(seek) = seek {
                if seek > 0.0 {
                    log::debug!("start_playback: Seeking track to seek={seek}");
                    self.seek_track(seek, Some(DEFAULT_SEEK_RETRY_OPTIONS))
                        .await?;
                }
            }

            crate::play(&self.service, self.device.url(), self.instance_id, 1.0)
                .await
                .map_err(|e| {
                    log::error!("play failed: {e:?}");
                    PlayerError::NoPlayersPlaying
                })?;

            self.expected_state
                .write()
                .unwrap()
                .replace("PLAYING".to_string());

            self.subscribe(
                start_tx,
                finished_tx,
                transport_uri,
                current_seek,
                sent_playback_start_event,
            )
            .await?;

            tokio::select! {
                _ = abort.cancelled() => {
                    log::debug!("playback cancelled");
                    self.unsubscribe().await?;
                }
                retry = finished_rx.recv_async() => {
                    self.unsubscribe().await?;
                    if !retry.is_ok_and(|x| !x) {
                        if let Some(retry_options) = retry_options{
                            retry_count += 1;
                            if retry_count > retry_options.max_retry_count {
                                log::error!(
                                    "Playback retry failed after {retry_count} attempts. Not retrying"
                                );
                                break;
                            }
                            continue;
                        } else {
                            log::debug!("No retry options");
                        }
                    }
                }
            };

            log::info!("Finished playback for track {}", track_id);
            break;
        }

        Ok(())
    }

    async fn stop(&self) -> Result<Playback, PlayerError> {
        log::info!("Stopping playback");
        let playback = self.get_playback()?;

        if !playback.playing {
            log::debug!("Playback not playing: {playback:?}");
            return Ok(playback);
        }

        if let Err(e) = self.wait_for_expected_transport_state().await {
            log::warn!("Playback not in a stoppable state: {e:?}");
        }
        crate::stop(&self.service, self.device.url(), self.instance_id)
            .await
            .map_err(|e| {
                log::error!("pause failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        self.expected_state
            .write()
            .unwrap()
            .replace("STOPPED".to_string());

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
        tracks: Option<Vec<TrackOrId>>,
        quality: Option<PlaybackQuality>,
        session_id: Option<usize>,
        session_playlist_id: Option<usize>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::debug!(
            "\
            update_playback:\n\t\
            source={:?}\n\t\
            modify_playback={modify_playback:?}\n\t\
            play={play:?}\n\t\
            stop={stop:?}\n\t\
            playing={playing:?}\n\t\
            position={position:?}\n\t\
            seek={seek:?}\n\t\
            volume={volume:?}\n\t\
            tracks={tracks:?}\n\t\
            quality={quality:?}\n\t\
            session_id={session_id:?}\
            {}
            ",
            self.source,
            std::backtrace::Backtrace::force_capture()
        );

        log::debug!("Waiting for play_lock...");
        let permit = self.play_lock.acquire().await?;
        log::debug!("Allowed to play");
        drop(permit);

        if stop.unwrap_or(false) {
            return Ok(PlaybackStatus {
                success: true,
                playback_id: self.stop().await?.id,
            });
        }

        let mut should_play = modify_playback && play.unwrap_or(false);
        let mut should_resume = false;
        let mut should_pause = false;
        let mut should_seek = false;

        let playback = if let Ok(playback) = self.get_playback() {
            log::trace!("update_playback: existing playback={playback:?}");

            if playback.playing {
                should_seek = modify_playback
                    && seek.is_some()
                    && same_active_track(position, tracks.as_deref(), &playback);
                should_play = should_play && !should_seek;
                if let Some(false) = playing {
                    should_pause = modify_playback;
                }
            } else if position.is_none() || tracks.is_some() {
                should_resume = modify_playback
                    && (seek.is_none()
                        && same_active_track(position, tracks.as_deref(), &playback)
                        && (should_resume || playing.unwrap_or(false)));
            }

            playback
        } else {
            log::trace!("update_playback: no existing playback");
            should_play = modify_playback
                && (should_play || (play.unwrap_or(true) && playing.unwrap_or(false)));

            Playback::new(
                tracks.clone().unwrap_or_default(),
                position,
                AtomicF64::new(volume.unwrap_or(1.0)),
                quality.unwrap_or_default(),
                session_id,
                session_playlist_id,
            )
        };

        log::debug!("update_playback: modify_playback={modify_playback} should_play={should_play} should_resume={should_resume} should_pause={should_pause} should_seek={should_seek}");

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
        let progress = if playback.progress != 0.0 {
            Some(playback.progress)
        } else {
            None
        };

        if should_resume {
            match self.resume_playback(retry_options).await {
                Ok(status) => Ok(status),
                Err(e) => {
                    log::error!("Failed to resume playback: {e:?}");
                    self.play_playback(playback, progress, retry_options).await
                }
            }
        } else if should_play {
            self.play_playback(playback, progress, retry_options).await
        } else {
            if should_pause {
                self.pause_playback().await?;
            } else if should_seek {
                if let Some(seek) = seek {
                    log::debug!("update_playback: Seeking track to seek={seek}");
                    self.seek_track(seek, Some(DEFAULT_SEEK_RETRY_OPTIONS))
                        .await?;
                }
            }
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

    async fn seek_track(
        &self,
        seek: f64,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::debug!("seek_track seek={seek}");
        let playback = self.get_playback()?;
        let playback_id = playback.id;

        let mut attempt = 1;

        while let Err(e) = crate::seek(
            &self.service,
            self.device.url(),
            self.instance_id,
            "ABS_TIME",
            seek as u32,
        )
        .await
        .map_err(|e| PlayerError::Seek(format!("{e:?}")))
        {
            log::debug!("Seek failed: {e:?}");
            if let Some(retry_options) = retry_options {
                if attempt < retry_options.max_retry_count {
                    attempt += 1;
                    log::debug!(
                        "Retrying attempt {attempt}/{}",
                        retry_options.max_retry_count
                    );
                    tokio::time::sleep(retry_options.retry_delay).await;
                    continue;
                }
            }
            moosicbox_assert::die_or_error!("Failed to seek: {e:?}");
            return Err(e);
        }

        Ok(PlaybackStatus {
            success: true,
            playback_id,
        })
    }

    async fn pause_playback(&self) -> Result<PlaybackStatus, PlayerError> {
        log::info!("pause_playback: pausing playback");
        let mut playback = self.get_playback()?;

        let id = playback.id;
        log::debug!("pause_playback: playback id={id}");

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        playback.playing = false;

        self.active_playback
            .clone()
            .write()
            .unwrap()
            .replace(playback);

        if let Err(e) = self.wait_for_expected_transport_state().await {
            log::error!("Playback not in a pauseable state: {e:?}");
            return Ok(PlaybackStatus {
                success: false,
                playback_id: id,
            });
        }
        crate::pause(&self.service, self.device.url(), self.instance_id)
            .await
            .map_err(|e| {
                log::error!("pause failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        self.expected_state
            .write()
            .unwrap()
            .replace("PAUSED_PLAYBACK".to_string());

        moosicbox_assert::die_or_propagate!(
            self.wait_for_expected_transport_state().await,
            "Failed to wait_for_transport_state",
        );
        log::debug!("pause_playback: playback paused id={id}");

        Ok(PlaybackStatus {
            success: true,
            playback_id: id,
        })
    }

    async fn resume_playback(
        &self,
        _retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Resuming playback id");
        let mut playback = self.get_playback()?;

        let id = playback.id;

        if playback.playing {
            log::debug!("Playback already playing");
            return Err(PlayerError::PlaybackAlreadyPlaying(id));
        }

        self.wait_for_expected_transport_state().await?;
        crate::play(&self.service, self.device.url(), self.instance_id, 1.0)
            .await
            .map_err(|e| {
                log::error!("resume failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        self.expected_state
            .write()
            .unwrap()
            .replace("PLAYING".to_string());

        self.wait_for_expected_transport_state().await?;
        log::debug!("resume_playback: playback resumed id={id}");

        playback.playing = true;

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

    async fn track_id_to_playable_stream(
        &self,
        track_id: i32,
        source: ApiSource,
        quality: PlaybackQuality,
    ) -> Result<PlayableTrack, PlayerError> {
        let (url, headers) = get_track_url(
            track_id.try_into().unwrap(),
            source,
            &self.source,
            quality,
            true,
        )
        .await?;

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
            track_id,
            source: Box::new(source),
            hint,
        })
    }

    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
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

    fn get_playback(&self) -> Result<Playback, PlayerError> {
        log::trace!("Getting Playback");
        if let Some(playback) = self.active_playback.read().unwrap().as_ref() {
            Ok(playback.clone())
        } else {
            Err(PlayerError::NoPlayersPlaying)
        }
    }
}

fn same_active_track(
    position: Option<u16>,
    tracks: Option<&[TrackOrId]>,
    playback: &Playback,
) -> bool {
    match (position, tracks) {
        (None, None) => true,
        (Some(position), None) => playback.position == position,
        (None, Some(tracks)) => {
            tracks
                .get(playback.position as usize)
                .map(|x: &TrackOrId| x.to_id())
                == playback
                    .tracks
                    .get(playback.position as usize)
                    .map(|x: &TrackOrId| x.to_id())
        }
        (Some(position), Some(tracks)) => {
            tracks.get(position as usize).map(|x: &TrackOrId| x.to_id())
                == playback
                    .tracks
                    .get(playback.position as usize)
                    .map(|x: &TrackOrId| x.to_id())
        }
    }
}

impl UpnpPlayer {
    pub fn new(
        db: Arc<Box<dyn Database>>,
        device: Device,
        service: Service,
        source: PlayerSource,
        handle: Handle,
    ) -> UpnpPlayer {
        UpnpPlayer {
            id: thread_rng().gen::<usize>(),
            db,
            source,
            transport_uri: Arc::new(tokio::sync::RwLock::new(None)),
            active_playback: Arc::new(RwLock::new(None)),
            play_lock: Arc::new(tokio::sync::Semaphore::new(1)),
            receiver: Arc::new(RwLock::new(None)),
            handle,
            device,
            service,
            instance_id: 1,
            expected_state: Arc::new(RwLock::new(None)),
            media_info_subscription_id: Arc::new(tokio::sync::RwLock::new(0)),
            position_info_subscription_id: Arc::new(tokio::sync::RwLock::new(0)),
            transport_info_subscription_id: Arc::new(tokio::sync::RwLock::new(0)),
        }
    }

    async fn wait_for_expected_transport_state(&self) -> Result<(), PlayerError> {
        let expected_state = self
            .expected_state
            .read()
            .unwrap()
            .clone()
            .ok_or(PlayerError::NoPlayersPlaying)?;

        self.wait_for_transport_state(&expected_state).await?;

        Ok(())
    }

    async fn wait_for_transport_state(&self, desired_state: &str) -> Result<(), PlayerError> {
        let mut state = "".to_owned();
        let mut attempt = 0;

        while state.as_str() != desired_state {
            let info =
                crate::get_transport_info(&self.service, self.device.url(), self.instance_id)
                    .await
                    .expect("failed to get transport info");

            log::debug!("Waiting for state={desired_state} (current info={info:?})",);

            info.current_transport_state.clone_into(&mut state);

            if attempt >= 10 {
                return Err(PlayerError::NoPlayersPlaying);
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            attempt += 1;
        }
        log::debug!("wait_for_transport_state: state={state}");
        Ok(())
    }

    async fn subscribe(
        &self,
        start_tx: Sender<()>,
        finished_tx: Sender<bool>,
        _track_url: String,
        current_seek: Arc<RwLock<Option<f64>>>,
        sent_playback_start_event: Arc<AtomicBool>,
    ) -> Result<(), PlayerError> {
        log::debug!("subscribe: Subscribing events");
        let unsubscribe = {
            let handle = self.handle.clone();
            let media_info_subscription_id = *self.media_info_subscription_id.read().await;
            let position_info_subscription_id = *self.position_info_subscription_id.read().await;
            let transport_info_subscription_id = *self.transport_info_subscription_id.read().await;

            move || {
                let handle = handle.clone();

                Self::unsubscribe_events(
                    &handle,
                    media_info_subscription_id,
                    position_info_subscription_id,
                    transport_info_subscription_id,
                )
            }
        };

        unsubscribe()?;

        let transport_sub = self
            .handle
            .subscribe_transport_info(
                Duration::from_millis(1000),
                self.instance_id,
                self.device.udn().to_owned(),
                self.service.service_id().to_owned(),
                Box::new({
                    |transport_info| {
                        Box::pin(async move {
                            log::debug!("transport_info={transport_info:?}");
                        })
                    }
                }),
            )
            .await
            .map_err(|e| {
                log::error!("subscribe_position_info failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        let position_sub = self
            .handle
            .subscribe_position_info(
                Duration::from_millis(1000),
                self.instance_id,
                self.device.udn().to_owned(),
                self.service.service_id().to_owned(),
                Box::new({
                    let active_playback = self.active_playback.clone();
                    let unsubscribe = unsubscribe.clone();
                    let transport_uri = self.transport_uri.read().await.clone();
                    move |position_info| {
                        let active_playback = active_playback.clone();
                        let unsubscribe = unsubscribe.clone();
                        let current_seek = current_seek.clone();
                        let start_tx = start_tx.clone();
                        let finished_tx = finished_tx.clone();
                        let sent_playback_start_event = sent_playback_start_event.clone();
                        let transport_uri = transport_uri.clone();

                        Box::pin(async move {
                            if log::log_enabled!(log::Level::Trace) {
                                log::debug!(
                                    "position_info={position_info:?} active_playback={:?}",
                                    active_playback.read().unwrap()
                                );
                            } else {
                                log::debug!("position_info={position_info:?}");
                            }
                            if position_info.track == 0
                                || transport_uri.as_ref().map(|x| xml::escape::escape_str_attribute(x).to_string()).is_some_and(|x| x != position_info.track_uri)
                            {
                                log::debug!(
                                    "playback finished. unsubscribing. track={} track_uri=(expected={:?} actual={:?})",
                                    position_info.track,
                                    transport_uri,
                                    Some(position_info.track_uri),
                                );
                                if let Err(e) = finished_tx.send_async(false).await {
                                    log::error!("send error: {e:?}");
                                }
                                if let Err(e) = unsubscribe() {
                                    log::error!("Failed to unsubscribe: {e:?}");
                                }
                                return;
                            }

                            if position_info.track_duration == 0 {
                                log::debug!("Waiting for track duration...");
                                return;
                            }

                            if !sent_playback_start_event.load(std::sync::atomic::Ordering::SeqCst)
                            {
                                if let Err(e) = start_tx.send_async(()).await {
                                    log::error!("send error: {e:?}");
                                }
                            }

                            let position = position_info.abs_time;

                            let mut binding = active_playback.write().unwrap();
                            if let Some(playback) = binding.as_mut() {
                                if !sent_playback_start_event
                                    .load(std::sync::atomic::Ordering::SeqCst)
                                {
                                    if let Some(session_id) = playback.session_id {
                                        sent_playback_start_event
                                            .store(true, std::sync::atomic::Ordering::SeqCst);

                                        let update = UpdateSession {
                                            session_id: session_id as i32,
                                            play: None,
                                            stop: None,
                                            name: None,
                                            active: None,
                                            playing: Some(true),
                                            position: None,
                                            seek: Some(position as f64),
                                            volume: None,
                                            playlist: None,
                                        };
                                        send_playback_event(&update, playback);
                                    }
                                }

                                let old = playback.clone();
                                playback.progress = position as f64;
                                current_seek.write().unwrap().replace(playback.progress);
                                trigger_playback_event(playback, &old);
                            }
                        })
                    }
                }),
            )
            .await
            .map_err(|e| {
                log::error!("subscribe_position_info failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        let media_sub = self
            .handle
            .subscribe_media_info(
                Duration::from_millis(1000),
                self.instance_id,
                self.device.udn().to_owned(),
                self.service.service_id().to_owned(),
                Box::new(|media| {
                    Box::pin(async move {
                        log::debug!("media={media:?}");
                    })
                }),
            )
            .await
            .map_err(|e| {
                log::error!("subscribe_media_info failed: {e:?}");
                PlayerError::NoPlayersPlaying
            })?;

        *self.media_info_subscription_id.write().await = media_sub;
        *self.position_info_subscription_id.write().await = position_sub;
        *self.transport_info_subscription_id.write().await = transport_sub;

        log::debug!("subscribe: Subscribed media_sub={media_sub} position_sub={position_sub} transport_sub={transport_sub}");

        Ok(())
    }

    fn unsubscribe_events(
        handle: &Handle,
        media_info_subscription_id: usize,
        position_info_subscription_id: usize,
        transport_info_subscription_id: usize,
    ) -> Result<(), PlayerError> {
        if let Err(e) = handle.unsubscribe(media_info_subscription_id) {
            log::error!("unsubscribe_media_info error: {e:?}");
        } else {
            log::debug!("unsubscribed media info");
        }
        if let Err(e) = handle.unsubscribe(position_info_subscription_id) {
            log::error!("unsubscribe_position_info error: {e:?}");
        } else {
            log::debug!("unsubscribed position info");
        }
        if let Err(e) = handle.unsubscribe(transport_info_subscription_id) {
            log::error!("unsubscribe_transport_info error: {e:?}");
        } else {
            log::debug!("unsubscribed transport info");
        }

        log::debug!("unsubscribe_events: unsubscribed");
        Ok(())
    }

    async fn unsubscribe(&self) -> Result<(), PlayerError> {
        Self::unsubscribe_events(
            &self.handle,
            *self.media_info_subscription_id.read().await,
            *self.position_info_subscription_id.read().await,
            *self.transport_info_subscription_id.read().await,
        )
    }
}
