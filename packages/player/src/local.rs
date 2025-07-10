#![allow(clippy::module_name_repetitions)]

use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use atomic_float::AtomicF64;

use async_trait::async_trait;
use flume::Receiver;
use moosicbox_audio_decoder::{AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::{AudioOutput, AudioOutputFactory, AudioWrite};
use moosicbox_music_api::models::TrackAudioQuality;
use moosicbox_music_models::TrackApiSource;
use moosicbox_session::models::UpdateSession;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use tokio_util::sync::CancellationToken;

use crate::{
    ApiPlaybackStatus, Playback, PlaybackHandler, PlaybackType, Player, PlayerError, PlayerSource,
    send_playback_event, symphonia::play_media_source_async, track_or_id_to_playable,
};

#[derive(Clone)]
pub struct LocalPlayer {
    pub id: u64,
    playback_type: PlaybackType,
    source: PlayerSource,
    pub output: Option<Arc<Mutex<AudioOutputFactory>>>,
    pub receiver: Arc<tokio::sync::RwLock<Option<Receiver<()>>>>,
    pub playback: Arc<RwLock<Option<Playback>>>,
    pub playback_handler: Arc<RwLock<Option<PlaybackHandler>>>,
    pub consumed_samples: Arc<AtomicUsize>,
    pub sample_rate: Arc<AtomicUsize>,
    pub channels: Arc<AtomicUsize>,
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
        let consumed_samples = self.consumed_samples.clone();
        let sample_rate = self.sample_rate.clone();
        let channels = self.channels.clone();
        let shared_volume = self.shared_volume.clone(); // Move outside closure

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

        // Start progress tracking task
        let progress_playback = active_playback.clone();
        let progress_consumed = consumed_samples.clone();
        let progress_sample_rate = sample_rate.clone();
        let progress_channels = channels.clone();
        let progress_abort = playback.abort.clone();
        let progress_output = self.output.clone(); // Need this to get CPAL output sample rate
        let initial_position = seek.unwrap_or(0.0);

        moosicbox_task::spawn("player: progress_tracker", async move {
            let mut last_position = initial_position;

            log::debug!(
                "Progress tracking task started with initial_position={initial_position:.2}s"
            );

            // Wait for audio output to be available (sample rate > 0)
            let mut wait_iterations = 0;
            loop {
                if progress_abort.is_cancelled() {
                    log::debug!("Progress tracking task: abort requested during initialization");
                    return;
                }

                let sample_rate_val = progress_sample_rate.load(Ordering::SeqCst);
                log::debug!(
                    "Progress tracking task: waiting for initialization, sample_rate={sample_rate_val}, iteration={wait_iterations}"
                );

                if sample_rate_val > 0 {
                    log::debug!(
                        "Progress tracking task: initialization complete, sample_rate={sample_rate_val}"
                    );
                    break;
                }

                wait_iterations += 1;
                if wait_iterations > 100 {
                    log::warn!(
                        "Progress tracking task: timeout waiting for initialization after 10 seconds"
                    );
                    return;
                }

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            log::debug!("Progress tracking task: starting main loop");

            while !progress_abort.is_cancelled() {
                let consumed = progress_consumed.load(Ordering::SeqCst);
                let input_sample_rate = progress_sample_rate.load(Ordering::SeqCst);
                let input_channels = progress_channels.load(Ordering::SeqCst);

                if input_sample_rate > 0 && input_channels > 0 {
                    // Use actual output sample rate for accurate progress calculation
                    #[allow(clippy::cast_precision_loss)]
                    let (actual_sample_rate, actual_channels) = progress_output
                        .as_ref()
                        .and_then(|output_guard| {
                            output_guard.lock().ok().and_then(|output| {
                                output.try_into_output().ok().and_then(|audio_output| {
                                    audio_output.get_output_spec().map(|output_spec| {
                                        (
                                            f64::from(output_spec.rate),
                                            output_spec.channels.count() as f64,
                                        )
                                    })
                                })
                            })
                        })
                        .unwrap_or((input_sample_rate as f64, input_channels as f64));

                    #[allow(clippy::cast_precision_loss)]
                    let position = consumed as f64 / (actual_sample_rate * actual_channels);

                    // Enhanced debug logging to diagnose sample rate issues
                    log::debug!(
                        "🔍 Progress tracking: consumed={consumed}, input_rate={input_sample_rate}, input_channels={input_channels}, cpal_rate={actual_sample_rate}, cpal_channels={actual_channels}, position={position:.2}s, last_position={last_position:.2}s"
                    );

                    // Calculate the time until the next second boundary for smooth visual updates
                    let fractional_part = position - position.floor();
                    let next_second_boundary = position.floor() + 1.0;
                    let time_to_next_second = next_second_boundary - position;

                    // If we're very close to a second boundary (within 50ms), update immediately
                    let should_update_now = time_to_next_second <= 0.05 || fractional_part <= 0.05;

                    // Only update if we're at a second boundary or position has changed significantly
                    let significant_change = (position - last_position).abs() > 0.01;

                    if should_update_now || significant_change {
                        last_position = position;

                        let old = {
                            let mut binding = progress_playback.write().unwrap();
                            if let Some(playback) = binding.as_mut() {
                                let old = playback.clone();
                                playback.progress = position;
                                Some(old)
                            } else {
                                None
                            }
                        };

                        // Trigger progress event
                        if let Some(old) = old {
                            if should_update_now {
                                log::debug!(
                                    "Progress tracking: second boundary update at position={position:.2}s (fractional={fractional_part:.3})"
                                );
                            } else {
                                log::debug!(
                                    "Progress tracking: significant change update at position={position:.2}s"
                                );
                            }

                            if let Some(playback_target) = old.playback_target.clone() {
                                let update = UpdateSession {
                                    session_id: old.session_id,
                                    profile: old.profile.clone(),
                                    playback_target,
                                    play: None,
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: None,
                                    position: None,
                                    seek: Some(position),
                                    volume: None,
                                    playlist: None,
                                    quality: None,
                                };
                                send_playback_event(&update, &old);
                            }
                        }
                    }
                }

                // Calculate precise sleep duration to hit the next second boundary
                let consumed = progress_consumed.load(Ordering::SeqCst);
                let input_sample_rate = progress_sample_rate.load(Ordering::SeqCst);
                let input_channels = progress_channels.load(Ordering::SeqCst);

                let sleep_duration = if input_sample_rate > 0 && input_channels > 0 {
                    // Get the actual output sample rate for accurate timing
                    #[allow(clippy::cast_precision_loss)]
                    let (actual_sample_rate, actual_channels) = progress_output
                        .as_ref()
                        .and_then(|output_guard| {
                            output_guard.lock().ok().and_then(|output| {
                                output.try_into_output().ok().and_then(|audio_output| {
                                    audio_output.get_output_spec().map(|output_spec| {
                                        (
                                            f64::from(output_spec.rate),
                                            output_spec.channels.count() as f64,
                                        )
                                    })
                                })
                            })
                        })
                        .unwrap_or((input_sample_rate as f64, input_channels as f64));

                    #[allow(clippy::cast_precision_loss)]
                    let current_position = consumed as f64 / (actual_sample_rate * actual_channels);
                    let fractional_part = current_position - current_position.floor();
                    let time_to_next_second = 1.0 - fractional_part;

                    // Ensure we don't sleep for more than 1 second or less than 10ms
                    let sleep_seconds = time_to_next_second.clamp(0.01, 1.0);

                    log::trace!(
                        "Progress tracking: calculated sleep duration={sleep_seconds:.3}s (current_pos={current_position:.3}s, fractional={fractional_part:.3}s, time_to_next={time_to_next_second:.3}s)"
                    );

                    std::time::Duration::from_secs_f64(sleep_seconds)
                } else {
                    // Fallback to 100ms if we don't have audio info yet
                    std::time::Duration::from_millis(100)
                };

                tokio::time::sleep(sleep_duration).await;
            }
        });

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
                    let consumed_samples = consumed_samples.clone();
                    let sample_rate = sample_rate.clone();
                    let channels = channels.clone();
                    let seek_position = seek.unwrap_or(0.0);
                    let shared_volume_local = shared_volume.clone();
                    move |spec, _duration| {
                        use moosicbox_audio_output::AudioWrite;

                        let mut output: AudioOutput = (open_func.lock().unwrap())
                            .try_into_output()
                            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;

                        // Store audio format info for progress tracking
                        log::debug!("🔍 Audio output creation: setting sample_rate={}, channels={}",
                            spec.rate, spec.channels.count());
                        sample_rate.store(spec.rate as usize, Ordering::SeqCst);
                        channels.store(spec.channels.count(), Ordering::SeqCst);

                        // Initialize consumed samples based on seek position
                        #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        let initial_consumed_samples = if seek_position > 0.0 {
                            (seek_position * f64::from(spec.rate) * spec.channels.count() as f64) as usize
                        } else {
                            0
                        };
                        consumed_samples.store(initial_consumed_samples, Ordering::SeqCst);
                        log::warn!("🔍 Audio output creation: initialized consumed_samples counter to {initial_consumed_samples} (seek_position={seek_position:.2}s, rate={}, channels={}, calculation={:.2}*{}*{})",
                            spec.rate, spec.channels.count(),
                            seek_position, spec.rate, spec.channels.count());

                        // Set the consumed samples counter on the audio output
                        output.set_consumed_samples(consumed_samples.clone());
                        log::debug!("Audio output creation: set consumed_samples counter on output");

                        // Pass the shared volume atomic to the audio output
                        // This allows the CPAL callback to read volume changes immediately
                        output.set_shared_volume(shared_volume_local.clone());
                        log::info!("Audio output creation: set shared volume reference");

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

        // Clear the audio info to stop progress tracking
        self.sample_rate.store(0, Ordering::SeqCst);
        self.channels.store(0, Ordering::SeqCst);
        self.consumed_samples.store(0, Ordering::SeqCst);

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
        // Get the current actual playback position from our progress tracking
        let progress = {
            let consumed_samples = self.consumed_samples.load(Ordering::SeqCst);
            let input_sample_rate = self.sample_rate.load(Ordering::SeqCst);
            let input_channels = self.channels.load(Ordering::SeqCst);

            #[allow(clippy::cast_precision_loss)]
            if input_sample_rate > 0 && input_channels > 0 {
                // Use actual output sample rate for accurate progress calculation
                let (actual_sample_rate, actual_channels) = self
                    .output
                    .as_ref()
                    .and_then(|output_guard| {
                        output_guard.lock().ok().and_then(|output| {
                            output.try_into_output().ok().and_then(|audio_output| {
                                audio_output.get_output_spec().map(|output_spec| {
                                    (
                                        f64::from(output_spec.rate),
                                        output_spec.channels.count() as f64,
                                    )
                                })
                            })
                        })
                    })
                    .unwrap_or((input_sample_rate as f64, input_channels as f64));

                // Calculate actual position from consumed samples using CPAL output sample rate
                consumed_samples as f64 / (actual_sample_rate * actual_channels)
            } else {
                // Fallback to stored progress if no audio info
                self.playback
                    .read()
                    .unwrap()
                    .as_ref()
                    .ok_or(PlayerError::NoPlayersPlaying)?
                    .progress
            }
        };

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
            consumed_samples: Arc::new(AtomicUsize::new(0)),
            sample_rate: Arc::new(AtomicUsize::new(0)),
            channels: Arc::new(AtomicUsize::new(0)),
            shared_volume,
        })
    }

    #[must_use]
    pub fn with_output(mut self, output: AudioOutputFactory) -> Self {
        self.output.replace(Arc::new(Mutex::new(output)));
        self
    }
}
