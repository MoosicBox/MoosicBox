#![allow(clippy::module_name_repetitions)]

use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use atomic_float::AtomicF64;

use async_trait::async_trait;
use flume::Receiver;
use moosicbox_audio_decoder::{AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::{AudioOutput, AudioOutputFactory};
use moosicbox_music_api::models::TrackAudioQuality;
use moosicbox_music_models::TrackApiSource;
use moosicbox_session::models::UpdateSession;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use tokio_util::sync::CancellationToken;

use crate::{
    ApiPlaybackStatus, Playback, PlaybackHandler, PlaybackType, Player, PlayerError, PlayerSource,
    send_playback_event, symphonia::play_media_source_async, track_or_id_to_playable,
};

#[derive(Debug, Clone)]
struct ProgressUpdate {
    current_position: f64,
    session_id: u64,
    profile: String,
    playback_target: moosicbox_session::models::PlaybackTarget,
}

#[derive(Clone)]
pub struct LocalPlayer {
    pub id: u64,
    playback_type: PlaybackType,
    source: PlayerSource,
    pub output: Option<Arc<Mutex<AudioOutputFactory>>>,
    pub receiver: Arc<tokio::sync::RwLock<Option<Receiver<()>>>>,
    pub playback: Arc<RwLock<Option<Playback>>>,
    pub playback_handler: Arc<RwLock<Option<PlaybackHandler>>>,
    pub shared_volume: Arc<AtomicF64>, // Shared volume for immediate audio output updates
}

impl std::fmt::Debug for LocalPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalPlayer")
            .field("id", &self.id)
            .field("playback_type", &self.playback_type)
            .field("source", &self.source)
            .field("output", &self.output)
            .field("receiver", &self.receiver)
            .field("playback", &self.playback)
            .field(
                "shared_volume",
                &self.shared_volume.load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Player for LocalPlayer {
    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    async fn after_update_playback(&self) -> Result<(), PlayerError> {
        // Sync current playback volume to shared volume atomic
        // This ensures audio output gets the correct volume immediately after update
        if let Some(playback) = self.playback.read().unwrap().as_ref() {
            let current_volume = playback.volume.load(std::sync::atomic::Ordering::SeqCst);
            self.shared_volume
                .store(current_volume, std::sync::atomic::Ordering::SeqCst);
            log::debug!(
                "🔊 LocalPlayer {}: synced volume to shared atomic after update: {:.3}",
                self.id,
                current_volume
            );
        }
        Ok(())
    }

    async fn before_play_playback(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        let playing = {
            self.playback
                .read()
                .unwrap()
                .as_ref()
                .ok_or(PlayerError::NoPlayersPlaying)?
                .playing
        };

        log::trace!("before_play_playback: playing={playing} seek={seek:?}");
        if playing {
            self.trigger_stop().await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        let Some(playback) = self.playback.read().unwrap().clone() else {
            return Err(PlayerError::NoPlayersPlaying);
        };

        let track = &playback.tracks[playback.position as usize];
        let track_id = &track.id;
        log::info!(
            "Playing track with Symphonia: {} {:?} {track:?}",
            track_id,
            playback.abort,
        );

        #[allow(clippy::match_wildcard_for_single_variants)]
        let playback_type = match track.track_source {
            TrackApiSource::Local => self.playback_type,
            #[allow(unreachable_patterns)]
            _ => PlaybackType::Stream,
        };

        let playable_track = track_or_id_to_playable(
            playback_type,
            track,
            playback.quality,
            TrackAudioQuality::Low,
            &self.source,
            playback.abort.clone(),
        )
        .await?;
        let mss =
            MediaSourceStream::new(playable_track.source, MediaSourceStreamOptions::default());

        let active_playback = self.playback.clone();
        let sent_playback_start_event = AtomicBool::new(false);
        let shared_volume = self.shared_volume.clone();

        if self.output.is_none() {
            return Err(PlayerError::NoAudioOutputs);
        }

        let open_func = self.output.clone().unwrap();

        // Initialize shared volume with the current playback volume
        let initial_volume = {
            active_playback
                .read()
                .unwrap()
                .as_ref()
                .map_or(1.0, |playback| {
                    playback.volume.load(std::sync::atomic::Ordering::SeqCst)
                })
        };

        shared_volume.store(initial_volume, std::sync::atomic::Ordering::SeqCst);
        log::info!(
            "LocalPlayer: initialized shared volume to {:.3} (from current playback)",
            initial_volume
        );

        let get_handler = move || {
            #[allow(unused_mut)]
            let mut audio_decode_handler = AudioDecodeHandler::new()
                .with_filter(Box::new({
                    let active_playback = active_playback.clone();
                    let initial_seek_position = seek.unwrap_or(0.0);
                    move |_decoded, _packet, _track| {
                        // Just send the initial playback start event, don't track progress here
                        if !sent_playback_start_event.load(std::sync::atomic::Ordering::SeqCst) {
                            let binding = active_playback.read().unwrap();
                            if let Some(playback) = binding.as_ref() {
                                if let Some(playback_target) = playback.playback_target.clone() {
                                    sent_playback_start_event
                                        .store(true, std::sync::atomic::Ordering::SeqCst);

                                    log::debug!("trigger_play: Sending initial playback start event with seek={initial_seek_position:.2}s");

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
                                        seek: Some(initial_seek_position),
                                        volume: None,
                                        playlist: None,
                                        quality: None,
                                    };
                                    send_playback_event(&update, playback);
                                }
                            }
                        }
                        Ok(())
                    }
                }))
                .with_output(Box::new({
                    let seek_position = seek.unwrap_or(0.0);
                    let shared_volume_local = shared_volume.clone();
                    let playback_for_callback = active_playback.clone();
                    move |spec, _duration| {
                        use moosicbox_audio_output::AudioWrite;

                        let mut output: AudioOutput = (open_func.lock().unwrap())
                            .try_into_output()
                            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;

                        log::debug!("🔍 Audio output creation: spec rate={}, channels={}",
                            spec.rate, spec.channels.count());

                        // Initialize consumed samples based on seek position for the AudioOutput
                        let consumed_samples = Arc::new(AtomicUsize::new(0));

                        #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        let initial_consumed_samples = if seek_position > 0.0 {
                            (seek_position * f64::from(spec.rate) * spec.channels.count() as f64) as usize
                        } else {
                            0
                        };
                        consumed_samples.store(initial_consumed_samples, Ordering::SeqCst);
                        log::debug!("Audio output creation: initialized consumed_samples to {initial_consumed_samples} (seek_position={seek_position:.2}s)");

                        // Set the consumed samples counter on the audio output
                        output.set_consumed_samples(consumed_samples);

                        // Pass the shared volume atomic to the audio output
                        output.set_shared_volume(shared_volume_local.clone());
                        log::info!("Audio output creation: set shared volume reference");

                        // Set up progress callback to handle progress events from AudioOutput
                        // Create a channel for progress updates to avoid calling async code from audio thread
                        let (progress_tx, progress_rx) = flume::unbounded::<ProgressUpdate>();

                        // Spawn a task to handle progress updates from the audio thread
                        let playback_for_handler = playback_for_callback.clone();
                        moosicbox_task::spawn("player: Progress handler", async move {
                            let mut last_reported_second: Option<u64> = None;

                            while let Ok(progress_update) = progress_rx.recv_async().await {
                                let old = {
                                    let mut binding = playback_for_handler.write().unwrap();
                                    if let Some(playback) = binding.as_mut() {
                                        let old = playback.clone();
                                        playback.progress = progress_update.current_position;
                                        Some(old)
                                    } else {
                                        None
                                    }
                                };

                                // Only trigger progress event when the second changes
                                if let Some(old) = old {
                                    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                                    let current_second = progress_update.current_position as u64;
                                    let should_send_update = last_reported_second != Some(current_second);

                                    if should_send_update {
                                        last_reported_second = Some(current_second);

                                        log::debug!(
                                            "Progress callback: position={:.2}s (from AudioOutput) - sending session update",
                                            progress_update.current_position
                                        );

                                        let update = UpdateSession {
                                            session_id: progress_update.session_id,
                                            profile: progress_update.profile,
                                            playback_target: progress_update.playback_target,
                                            play: None,
                                            stop: None,
                                            name: None,
                                            active: None,
                                            playing: None,
                                            position: None,
                                            seek: Some(progress_update.current_position),
                                            volume: None,
                                            playlist: None,
                                            quality: None,
                                        };
                                        send_playback_event(&update, &old);
                                    } else {
                                        log::trace!(
                                            "Progress callback: position={:.2}s (from AudioOutput) - skipping session update (same second)",
                                            progress_update.current_position
                                        );
                                    }
                                }
                            }
                        });

                        let progress_callback = {
                            let playback_ref = playback_for_callback.clone();
                            Box::new(move |current_position: f64| {
                                // Get the current playback info to send with the progress update
                                let playback_info = {
                                    let binding = playback_ref.read().unwrap();
                                    binding.as_ref().and_then(|playback| {
                                        playback.playback_target.clone().map(|target| {
                                            ProgressUpdate {
                                                current_position,
                                                session_id: playback.session_id,
                                                profile: playback.profile.clone(),
                                                playback_target: target,
                                            }
                                        })
                                    })
                                };

                                // Send progress update through channel to avoid async calls from audio thread
                                if let Some(progress_info) = playback_info {
                                    if let Err(e) = progress_tx.send(progress_info) {
                                        log::error!("Failed to send progress update: {e}");
                                    }
                                }
                            })
                        };

                        output.set_progress_callback(Some(progress_callback));
                        log::debug!("Audio output creation: set progress callback");

                        Ok(Box::new(output))
                    }
                }))
                .with_cancellation_token(playback.abort);

            moosicbox_assert::assert_or_err!(
                audio_decode_handler.contains_outputs_to_open(),
                crate::symphonia::PlaybackError::NoAudioOutputs,
                "No outputs set for the audio_decode_handler"
            );

            Ok(audio_decode_handler)
        };

        let response = play_media_source_async(
            mss,
            &playable_track.hint,
            get_handler,
            true,
            true,
            None,
            seek,
        )
        .await;

        if let Err(e) = response {
            log::error!("Failed to play playback: {e:?}");
            return Err(e.into());
        }

        log::info!("Finished playback for track_id={}", track_id);

        Ok(())
    }

    async fn trigger_stop(&self) -> Result<(), PlayerError> {
        log::info!("Stopping playback");

        {
            let Some(playback) = self.playback.read().unwrap().clone() else {
                return Err(PlayerError::NoPlayersPlaying);
            };

            log::debug!("Aborting playback {playback:?} for stop");
            playback.abort.cancel();

            log::trace!("Waiting for playback completion response");
            let receiver = self.receiver.write().await.take();
            if let Some(receiver) = receiver {
                tokio::select! {
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
                    () = tokio::time::sleep(std::time::Duration::from_millis(5000)) => {
                        log::error!("Playback timed out waiting for abort completion");
                    }
                }
            } else {
                log::debug!("No receiver to wait for completion response with");
            }
        }

        // Progress tracking is now handled by AudioOutput implementations

        self.playback.write().unwrap().as_mut().unwrap().abort = CancellationToken::new();

        Ok(())
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError> {
        log::info!("Pausing playback id");
        {
            let Some(playback) = self.playback.read().unwrap().clone() else {
                return Err(PlayerError::NoPlayersPlaying);
            };

            let id = playback.id;

            log::info!("Aborting playback id {id} for pause");
            playback.abort.cancel();

            log::trace!("Waiting for playback completion response");
            let receiver = self.receiver.write().await.take();
            if let Some(receiver) = receiver {
                if let Err(err) = receiver.recv_async().await {
                    log::trace!("Sender correlated with receiver has dropped: {err:?}");
                }
            } else {
                log::debug!("No receiver to wait for completion response with");
            }
            log::trace!("Playback successfully paused");
        }

        // Don't clear audio info on pause - keep the progress tracking alive at the paused position
        // The progress tracking task will continue to show the current position

        self.playback.write().unwrap().as_mut().unwrap().abort = CancellationToken::new();

        Ok(())
    }

    async fn trigger_resume(&self) -> Result<(), PlayerError> {
        // Get the current playback position from stored progress
        // AudioOutput will handle accurate progress tracking via callbacks
        let progress = self
            .playback
            .read()
            .unwrap()
            .as_ref()
            .ok_or(PlayerError::NoPlayersPlaying)?
            .progress;

        log::info!("Resuming playback from position: {:.2}s", progress);

        let mut playback_handler = { self.playback_handler.read().unwrap().clone().unwrap() };
        playback_handler.play_playback(Some(progress), None).await?;

        Ok(())
    }

    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError> {
        let playing = {
            self.playback
                .read()
                .unwrap()
                .as_ref()
                .ok_or(PlayerError::NoPlayersPlaying)?
                .playing
        };

        if !playing {
            return Ok(());
        }

        let mut playback_handler = { self.playback_handler.read().unwrap().clone().unwrap() };
        playback_handler.play_playback(Some(seek), None).await?;

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

impl LocalPlayer {
    /// # Errors
    ///
    /// * If failed to generate a `LocalPlayer` `id`
    pub async fn new(
        source: PlayerSource,
        playback_type: Option<PlaybackType>,
    ) -> Result<Self, PlayerError> {
        let id = moosicbox_task::spawn_blocking("player: local player rng", || {
            switchy_random::rng().next_u64()
        })
        .await?;

        let shared_volume = Arc::new(AtomicF64::new(1.0));
        log::info!("LocalPlayer {id}: initialized shared volume to 1.0 (full volume)");

        Ok(Self {
            id,
            playback_type: playback_type.unwrap_or_default(),
            source,
            output: None,
            playback: Arc::new(RwLock::new(None)),
            receiver: Arc::new(tokio::sync::RwLock::new(None)),
            playback_handler: Arc::new(RwLock::new(None)),
            shared_volume,
        })
    }

    #[must_use]
    pub fn with_output(mut self, output: AudioOutputFactory) -> Self {
        self.output.replace(Arc::new(Mutex::new(output)));
        self
    }
}
