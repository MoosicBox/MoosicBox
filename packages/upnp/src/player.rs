use std::{
    sync::{atomic::AtomicBool, Arc, RwLock, RwLockWriteGuard},
    time::Duration,
};

use async_trait::async_trait;
use crossbeam_channel::Receiver;
use flume::{unbounded, Sender};
use moosicbox_core::sqlite::{
    db::get_track,
    models::{ToApi, UpdateSession},
};
use moosicbox_database::Database;
use rand::{thread_rng, Rng as _};
use rupnp::{Device, Service};

use moosicbox_player::player::{
    get_track_url, send_playback_event, trigger_playback_event, ApiPlaybackStatus, Playback,
    PlaybackRetryOptions, Player, PlayerError, PlayerSource,
};

use crate::listener::Handle;

pub const DEFAULT_SEEK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
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
    position_info_subscription_id: Arc<tokio::sync::RwLock<usize>>,
    expected_state: Arc<RwLock<Option<String>>>,
}

#[async_trait]
impl Player for UpnpPlayer {
    fn active_playback_write(&self) -> RwLockWriteGuard<'_, Option<Playback>> {
        self.active_playback.write().unwrap()
    }

    fn receiver_write(&self) -> RwLockWriteGuard<'_, Option<Receiver<()>>> {
        self.receiver.write().unwrap()
    }

    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        let current_seek = Arc::new(RwLock::new(seek));

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
        log::debug!("trigger_play: Set transport_uri={transport_uri}");
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
                log::debug!("trigger_play: Seeking track to seek={seek}");
                self.seek(seek, Some(DEFAULT_SEEK_RETRY_OPTIONS)).await?;
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
                    return Err(PlayerError::NoPlayersPlaying);
                }
            }
        };

        Ok(())
    }

    async fn trigger_stop(&self) -> Result<Playback, PlayerError> {
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
                log::error!("stop failed: {e:?}");
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

    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        log::debug!("Waiting for play_lock...");
        let permit = self.play_lock.acquire().await?;
        log::debug!("Allowed to play");
        drop(permit);

        Ok(())
    }

    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError> {
        log::debug!("seek_track seek={seek}");

        crate::seek(
            &self.service,
            self.device.url(),
            self.instance_id,
            "ABS_TIME",
            seek as u32,
        )
        .await
        .map_err(|e| PlayerError::Seek(format!("{e:?}")))?;

        Ok(())
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError> {
        log::info!("trigger_pause: pausing playback");
        let mut playback = self.get_playback()?;

        let id = playback.id;
        log::debug!("trigger_pause: playback id={id}");

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
            return Ok(());
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
        log::debug!("trigger_pause: playback paused id={id}");

        Ok(())
    }

    async fn trigger_resume(&self) -> Result<(), PlayerError> {
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
        log::debug!("trigger_resume: playback resumed id={id}");

        playback.playing = true;

        self.active_playback
            .clone()
            .write()
            .unwrap()
            .replace(playback);

        Ok(())
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

    fn get_source(&self) -> &PlayerSource {
        &self.source
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
            position_info_subscription_id: Arc::new(tokio::sync::RwLock::new(0)),
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
            let position_info_subscription_id = *self.position_info_subscription_id.read().await;

            move || {
                let handle = handle.clone();

                Self::unsubscribe_events(&handle, position_info_subscription_id)
            }
        };

        unsubscribe()?;

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

        *self.position_info_subscription_id.write().await = position_sub;

        log::debug!("subscribe: Subscribed position_sub={position_sub}");

        Ok(())
    }

    fn unsubscribe_events(
        handle: &Handle,
        position_info_subscription_id: usize,
    ) -> Result<(), PlayerError> {
        if let Err(e) = handle.unsubscribe(position_info_subscription_id) {
            log::error!("unsubscribe_position_info error: {e:?}");
        } else {
            log::debug!("unsubscribed position info");
        }

        log::debug!("unsubscribe_events: unsubscribed");
        Ok(())
    }

    async fn unsubscribe(&self) -> Result<(), PlayerError> {
        Self::unsubscribe_events(
            &self.handle,
            *self.position_info_subscription_id.read().await,
        )
    }
}
