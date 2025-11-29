//! `UPnP` player implementation for controlling media playback on UPnP/DLNA devices.
//!
//! This module provides a [`Player`] implementation that controls
//! playback on UPnP/DLNA devices. Requires the `player` feature to be enabled.
//!
//! [`Player`]: moosicbox_player::Player

#![allow(clippy::module_name_repetitions)]

use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::Duration,
};

use async_trait::async_trait;
use flume::{Receiver, Sender, unbounded};
use moosicbox_async_service::CancellationToken;
use moosicbox_audio_output::{
    AudioOutputError, AudioOutputFactory, AudioWrite, Channels, SignalSpec,
};
use moosicbox_music_api::{SourceToMusicApi, models::TrackAudioQuality};
use moosicbox_session::models::UpdateSession;
use rupnp::{Device, Service};
use switchy_async::sync::RwLock as AsyncRwLock;

use moosicbox_player::{
    ApiPlaybackStatus, Playback, PlaybackHandler, PlaybackRetryOptions, Player, PlayerError,
    PlayerSource, get_track_url, send_playback_event, trigger_playback_event,
};
use symphonia::core::audio::AudioBuffer;

use crate::listener::Handle;

/// Default retry options for seeking operations on `UPnP` devices.
pub const DEFAULT_SEEK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: std::time::Duration::from_millis(100),
};

/// `UPnP` player implementation that controls playback on `UPnP`/DLNA devices.
#[derive(Clone)]
pub struct UpnpPlayer {
    /// Music API provider for retrieving track information and URLs.
    pub source_to_music_api: Arc<Box<dyn SourceToMusicApi + Send + Sync>>,
    /// Unique identifier for this player instance.
    pub id: u64,
    source: PlayerSource,
    transport_uri: Arc<AsyncRwLock<Option<String>>>,
    /// Current playback state and information.
    pub playback: Arc<RwLock<Option<Playback>>>,
    /// Handler for managing playback operations.
    pub playback_handler: Arc<RwLock<Option<PlaybackHandler>>>,
    /// Receiver for playback completion notifications.
    pub receiver: Arc<AsyncRwLock<Option<Receiver<()>>>>,
    handle: Handle,
    /// The `UPnP` device being controlled.
    pub device: Device,
    service: Service,
    instance_id: u32,
    position_info_subscription_id: Arc<AsyncRwLock<usize>>,
    expected_state: Arc<RwLock<Option<String>>>,
}

impl std::fmt::Debug for UpnpPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpnpPlayer")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("transport_uri", &self.transport_uri)
            .field("playback", &self.playback)
            .field("receiver", &self.receiver)
            .field("device", &self.device)
            .field("service", &self.service)
            .field("instance_id", &self.instance_id)
            .field(
                "position_info_subscription_id",
                &self.position_info_subscription_id,
            )
            .field("expected_state", &self.expected_state)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Player for UpnpPlayer {
    async fn before_play_playback(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        let playing = {
            self.playback
                .read()
                .unwrap()
                .as_ref()
                .ok_or(PlayerError::NoPlayersPlaying)?
                .playing
        };
        log::trace!("before_play_playback: playing={playing:?} seek={seek:?}");
        if playing {
            log::trace!("before_play_playback: Aborting existing playback");
            let mut binding = self.playback.write().unwrap();
            let playback = binding.as_mut().unwrap();
            playback.abort.cancel();
            playback.abort = CancellationToken::new();
            drop(binding);
        }

        Ok(())
    }

    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        log::debug!("trigger_play: seek={seek:?}");
        let transport_uri = self.update_av_transport().await?;

        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

        if let Some(seek) = seek
            && seek > 0.0
        {
            log::debug!("trigger_play: Seeking track to seek={seek}");
            self.trigger_seek(seek).await?;
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

        let (finished_tx, finished_rx) = unbounded();
        let sent_playback_start_event = Arc::new(AtomicBool::new(false));

        let sub_id = self
            .subscribe(
                finished_tx,
                transport_uri,
                Arc::new(RwLock::new(seek)),
                sent_playback_start_event,
            )
            .await?;

        switchy_async::select! {
            () = playback.abort.cancelled() => {
                log::debug!("playback cancelled");
                self.unsubscribe(sub_id);
            }
            retry = finished_rx.recv_async() => {
                self.unsubscribe(sub_id);
                match retry {
                    Ok(false) => {
                        log::debug!("Playback finished and retry wasn't requested");
                    }
                    Ok(true) => {
                        log::debug!("Retrying playback");
                        return Err(PlayerError::RetryRequested);
                    }
                    Err(_e) => {
                        log::debug!("Playback end requested");
                    }
                }
            }
        };

        Ok(())
    }

    async fn trigger_stop(&self) -> Result<(), PlayerError> {
        log::info!("Stopping playback");
        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

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

        log::trace!("Waiting for playback completion response");
        let receiver = self.receiver.write().await.take();
        if let Some(receiver) = receiver {
            switchy_async::select! {
                resp = receiver.recv_async() => {
                    match resp {
                        Ok(()) => {
                            log::trace!("Playback successfully stopped");
                        }
                        Err(e) => {
                            log::info!("Sender associated with playback disconnected: {e:?}");
                        }
                    }
                }
                () = switchy_async::time::sleep(std::time::Duration::from_millis(5000)) => {
                    log::error!("Playback timed out waiting for abort completion");
                }
            }
        } else {
            log::debug!("No receiver to wait for completion response with");
        }

        self.playback.write().unwrap().as_mut().unwrap().abort = CancellationToken::new();

        Ok(())
    }

    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError> {
        log::info!("trigger_seek: seek={seek}");

        if self.expected_state.read().unwrap().is_none() {
            log::debug!("trigger_seek: State not set. Initializing AV Transport URI");
            self.update_av_transport().await?;
            self.init_transport_state().await?;
        }
        if self.expected_state.read().unwrap().as_deref() == Some("STOPPED") {
            log::debug!("trigger_seek: In STOPPED state. not seeking");
            return Ok(());
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
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
        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

        let id = playback.id;
        log::debug!("trigger_pause: playback id={id}");

        if let Err(e) = self.wait_for_transport_state("PLAYING").await {
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
        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

        let id = playback.id;

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

        Ok(())
    }

    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError> {
        Ok(ApiPlaybackStatus {
            active_playbacks: self
                .playback
                .clone()
                .read()
                .unwrap()
                .clone()
                .map(Into::into),
        })
    }

    fn get_source(&self) -> &PlayerSource {
        &self.source
    }
}

impl UpnpPlayer {
    /// Creates a new `UPnP` player instance for controlling a `UPnP` device.
    #[must_use]
    pub fn new(
        source_to_music_api: Arc<Box<dyn SourceToMusicApi + Send + Sync>>,
        device: Device,
        service: Service,
        source: PlayerSource,
        handle: Handle,
    ) -> Self {
        Self {
            id: switchy_random::rng().next_u64(),
            source_to_music_api,
            source,
            transport_uri: Arc::new(AsyncRwLock::new(None)),
            playback: Arc::new(RwLock::new(None)),
            playback_handler: Arc::new(RwLock::new(None)),
            receiver: Arc::new(AsyncRwLock::new(None)),
            handle,
            device,
            service,
            instance_id: 1,
            expected_state: Arc::new(RwLock::new(None)),
            position_info_subscription_id: Arc::new(AsyncRwLock::new(0)),
        }
    }

    async fn update_av_transport(&self) -> Result<String, PlayerError> {
        log::debug!("update_av_transport");
        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

        let track = &playback.tracks[playback.position as usize];
        let track_id = &track.id;
        log::info!(
            "update_av_transport: Updating UPnP AV Transport URI: {} {:?} {track:?}",
            track_id,
            playback.abort,
        );

        let (transport_uri, _) = get_track_url(
            track_id,
            &track.api_source,
            &self.source,
            playback.quality,
            TrackAudioQuality::FlacHighestRes,
            true,
        )
        .await?;

        self.transport_uri
            .write()
            .await
            .replace(transport_uri.clone());

        log::debug!("update_av_transport: Set transport_uri={transport_uri}");
        let format = "flac";

        let (local_transport_uri, headers) = get_track_url(
            track_id,
            &track.api_source,
            &self.source,
            playback.quality,
            TrackAudioQuality::FlacHighestRes,
            false,
        )
        .await?;

        let size = if matches!(
            switchy_env::var("UPNP_SEND_SIZE").as_deref(),
            Ok("1" | "true")
        ) {
            let mut client = switchy_http::Client::new().head(&local_transport_uri);

            if let Some(headers) = headers {
                for (key, value) in headers {
                    client = client.header(&key, &value);
                }
            }

            let mut res = client.send().await.unwrap();
            let headers = res.headers();
            headers
                .get("content-length")
                .map(|length| length.parse::<u64>().unwrap())
        } else {
            None
        };
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let duration = track.duration.ceil() as u32;
        let title = track.title.clone();
        let artist = track.artist.clone();
        let album = track.album.clone();
        let track_number = track.number;

        crate::set_av_transport_uri(
            &self.service,
            self.device.url(),
            self.instance_id,
            &transport_uri,
            format,
            Some(title.as_str()),
            Some(artist.as_str()),
            Some(artist.as_str()),
            Some(album.as_str()),
            Some(track_number),
            Some(duration),
            size,
        )
        .await
        .map_err(|e| {
            log::error!("set_av_transport_uri failed: {e:?}");
            PlayerError::InvalidState
        })?;

        Ok(transport_uri)
    }

    async fn init_transport_state(&self) -> Result<(), PlayerError> {
        let transport_info =
            crate::get_transport_info(&self.service, self.device.url(), self.instance_id)
                .await
                .map_err(|e| {
                    log::error!("get_transport_info failed: {e:?}");
                    PlayerError::InvalidState
                })?;

        log::trace!("update_av_transport: transport_info={transport_info:?}");

        self.expected_state
            .write()
            .unwrap()
            .replace(transport_info.current_transport_state);

        Ok(())
    }

    async fn wait_for_expected_transport_state(&self) -> Result<(), PlayerError> {
        let expected_state = self.expected_state.read().unwrap().clone().ok_or_else(|| {
            log::error!("State not set");
            PlayerError::InvalidState
        })?;

        self.wait_for_transport_state(&expected_state).await?;

        Ok(())
    }

    async fn wait_for_transport_state(&self, desired_state: &str) -> Result<(), PlayerError> {
        let mut state = String::new();
        let mut attempt = 0;

        while state.as_str() != desired_state {
            let info =
                crate::get_transport_info(&self.service, self.device.url(), self.instance_id)
                    .await
                    .expect("failed to get transport info");

            log::debug!("Waiting for state={desired_state} (current info={info:?})",);

            info.current_transport_state.clone_into(&mut state);

            if attempt >= 10 {
                log::error!("Failed to wait for transport_state to be {desired_state}");
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
        finished_tx: Sender<bool>,
        _track_url: String,
        current_seek: Arc<RwLock<Option<f64>>>,
        sent_playback_start_event: Arc<AtomicBool>,
    ) -> Result<usize, PlayerError> {
        log::debug!("subscribe: Subscribing events");

        let this_sub = Arc::new(AtomicUsize::new(0));

        let position_sub = self
            .handle
            .subscribe_position_info(
                Duration::from_millis(1000),
                self.instance_id,
                self.device.udn().to_owned(),
                self.service.service_id().to_owned(),
                Box::new({
                    let active_playback = self.playback.clone();
                    let transport_uri = self.transport_uri.read().await.clone();
                    let handle = self.handle.clone();
                    let this_sub = this_sub.clone();
                    move |position_info| {
                        let active_playback = active_playback.clone();
                        let current_seek = current_seek.clone();
                        let finished_tx = finished_tx.clone();
                        let sent_playback_start_event = sent_playback_start_event.clone();
                        let transport_uri = transport_uri.clone();
                        let handle = handle.clone();
                        let this_sub = this_sub.clone();

                        Box::pin(async move {
                            moosicbox_logging::debug_or_trace!(
                                ("position_info={position_info:?}"),
                                ("position_info={position_info:?} active_playback={:?}", active_playback.read().unwrap())
                            );
                            if position_info.track == 0
                                || transport_uri.as_ref().map(|x| xml::escape::escape_str_attribute(x).to_string()).is_some_and(|x| x != position_info.track_uri)
                            {
                                let sub_id = this_sub.load(std::sync::atomic::Ordering::SeqCst);
                                log::debug!(
                                    "playback finished. unsubscribing position_sub={sub_id}. track={} track_uri=(expected={:?} actual={:?})",
                                    position_info.track,
                                    transport_uri,
                                    Some(position_info.track_uri),
                                );
                                if let Err(e) = finished_tx.send_async(false).await {
                                    log::trace!("send error: {e:?}");
                                }
                                Self::unsubscribe_events(
                                    &handle,
                                    sub_id
                                );
                                return;
                            }

                            if position_info.track_duration == 0 {
                                log::debug!("Waiting for track duration...");
                                return;
                            }

                            let position = position_info.abs_time;

                            let mut binding = active_playback.write().unwrap();
                            if let Some(playback) = binding.as_mut() {
                                if !sent_playback_start_event
                                    .load(std::sync::atomic::Ordering::SeqCst)
                                    && let Some(playback_target) = playback.playback_target.clone() {
                                        sent_playback_start_event
                                            .store(true, std::sync::atomic::Ordering::SeqCst);

                                        let update = UpdateSession {
                                            session_id: playback.session_id,
                                            profile: playback.profile.clone(),
                                            playback_target,
                                            play: None,
                                            stop: None,
                                            name: None,
                                            active: None,
                                            playing: Some(true),
                                            position: None,
                                            seek: Some(f64::from(position)),
                                            volume: None,
                                            playlist: None,
                                            quality: None,
                                        };
                                        send_playback_event(&update, playback);
                                    }

                                let old = playback.clone();
                                playback.progress = f64::from(position);
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

        this_sub.store(position_sub, std::sync::atomic::Ordering::SeqCst);
        *self.position_info_subscription_id.write().await = position_sub;

        log::debug!("subscribe: Subscribed position_sub={position_sub}");

        Ok(position_sub)
    }

    fn unsubscribe_events(handle: &Handle, position_info_subscription_id: usize) {
        if let Err(e) = handle.unsubscribe(position_info_subscription_id) {
            log::error!("unsubscribe_position_info error: {e:?}");
        } else {
            log::debug!("unsubscribed position info");
        }

        log::debug!("unsubscribe_events: unsubscribed");
    }

    fn unsubscribe(&self, position_info_subscription_id: usize) {
        Self::unsubscribe_events(&self.handle, position_info_subscription_id);
    }
}

impl AudioWrite for UpnpPlayer {
    fn write(&mut self, _decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        unimplemented!("UpnpPlayer AudioWrite write is not implemented")
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        unimplemented!("UpnpPlayer AudioWrite flush is not implemented")
    }

    fn handle(&self) -> moosicbox_audio_output::AudioHandle {
        unimplemented!("UpnpPlayer does not support command handling")
    }
}

impl TryFrom<UpnpPlayer> for AudioOutputFactory {
    type Error = AudioOutputError;

    fn try_from(player: UpnpPlayer) -> Result<Self, Self::Error> {
        let name = player.device.friendly_name().to_string();
        let udn = player.device.udn();
        let spec = SignalSpec {
            rate: 384_000,
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        };

        let id = format!("upnp:{udn}");

        Ok(Self::new(id, name, spec, move || {
            Ok(Box::new(player.clone()))
        }))
    }
}

/// Represents a `UPnP` `AVTransport` service for audio output.
#[derive(Clone)]
pub struct UpnpAvTransportService {
    /// The `UPnP` device providing the `AVTransport` service.
    pub device: Device,
    /// The `AVTransport` service instance.
    pub service: Service,
}

impl AudioWrite for UpnpAvTransportService {
    fn write(&mut self, _decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        unimplemented!("UpnpAvTransportService AudioWrite write is not implemented")
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        unimplemented!("UpnpAvTransportService AudioWrite flush is not implemented")
    }

    fn handle(&self) -> moosicbox_audio_output::AudioHandle {
        unimplemented!("UpnpAvTransportService does not support command handling")
    }
}

impl TryFrom<UpnpAvTransportService> for AudioOutputFactory {
    type Error = AudioOutputError;

    fn try_from(player: UpnpAvTransportService) -> Result<Self, Self::Error> {
        let name = player.device.friendly_name().to_string();
        let udn = player.device.udn();
        let spec = SignalSpec {
            rate: 384_000,
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        };

        let id = format!("upnp:{udn}");

        Ok(Self::new(id, name, spec, move || {
            Ok(Box::new(player.clone()))
        }))
    }
}
