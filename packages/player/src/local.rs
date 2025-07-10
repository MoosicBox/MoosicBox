#![allow(clippy::module_name_repetitions)]

use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

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
    volume_mixer::mix_volume,
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
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Player for LocalPlayer {
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

        if self.output.is_none() {
            return Err(PlayerError::NoAudioOutputs);
        }

        let open_func = self.output.clone().unwrap();

        // Start progress tracking task
        let progress_playback = active_playback.clone();
        let progress_consumed = consumed_samples.clone();
        let progress_sample_rate = sample_rate.clone();
        let progress_channels = channels.clone();
        let progress_abort = playback.abort.clone();
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

            // Track progress while playback is active
            let mut first_calculation = true;
            while !progress_abort.is_cancelled() {
                let consumed = progress_consumed.load(Ordering::SeqCst);
                let sample_rate_val = progress_sample_rate.load(Ordering::SeqCst);
                let channels_val = progress_channels.load(Ordering::SeqCst);

                if sample_rate_val > 0 && channels_val > 0 {
                    #[allow(clippy::cast_precision_loss)]
                    let position = consumed as f64 / (sample_rate_val as f64 * channels_val as f64);

                    // Debug logging
                    log::debug!(
                        "Progress tracking: consumed={consumed}, sample_rate={sample_rate_val}, channels={channels_val}, position={position:.2}s, last_position={last_position:.2}s"
                    );

                    if first_calculation {
                        log::debug!(
                            "Progress tracking task: first position calculation - position={position:.2}s, last_position={last_position:.2}s"
                        );
                        first_calculation = false;
                    }

                    // Only update if position has changed significantly (avoid spam)
                    if (position - last_position).abs() > 0.01 {
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
                            log::debug!(
                                "Progress tracking: triggering progress event with position={position:.2}s"
                            );
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

                // Poll every 500ms for smooth progress updates
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
                .with_filter(Box::new(move |decoded, _packet, _track| {
                    mix_volume(
                        decoded,
                        playback.volume.load(std::sync::atomic::Ordering::SeqCst),
                    );
                    Ok(())
                }))
                .with_output(Box::new({
                    let consumed_samples = consumed_samples.clone();
                    let sample_rate = sample_rate.clone();
                    let channels = channels.clone();
                    let seek_position = seek.unwrap_or(0.0);
                    move |spec, _duration| {
                        use moosicbox_audio_output::AudioWrite;

                        let mut output: AudioOutput = (open_func.lock().unwrap())
                            .try_into_output()
                            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;

                        // Store audio format info for progress tracking
                        log::debug!("Audio output creation: setting sample_rate={}, channels={}",
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
                        log::debug!("Audio output creation: initialized consumed_samples counter to {initial_consumed_samples} (seek_position={seek_position:.2}s)");

                        // Set the consumed samples counter on the audio output
                        output.set_consumed_samples(consumed_samples.clone());
                        log::debug!("Audio output creation: set consumed_samples counter on output");

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
            let sample_rate = self.sample_rate.load(Ordering::SeqCst);
            let channels = self.channels.load(Ordering::SeqCst);

            #[allow(clippy::cast_precision_loss)]
            if sample_rate > 0 && channels > 0 {
                // Calculate actual position from consumed samples
                consumed_samples as f64 / (sample_rate * channels) as f64
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
        Ok(Self {
            id: moosicbox_task::spawn_blocking("player: local player rng", || {
                switchy_random::rng().next_u64()
            })
            .await?,
            playback_type: playback_type.unwrap_or_default(),
            source,
            output: None,
            playback: Arc::new(RwLock::new(None)),
            receiver: Arc::new(tokio::sync::RwLock::new(None)),
            playback_handler: Arc::new(RwLock::new(None)),
            consumed_samples: Arc::new(AtomicUsize::new(0)),
            sample_rate: Arc::new(AtomicUsize::new(0)),
            channels: Arc::new(AtomicUsize::new(0)),
        })
    }

    #[must_use]
    pub fn with_output(mut self, output: AudioOutputFactory) -> Self {
        self.output.replace(Arc::new(Mutex::new(output)));
        self
    }
}
