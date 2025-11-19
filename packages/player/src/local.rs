//! Local audio player implementation.
//!
//! This module provides a local player implementation that uses the Symphonia decoder
//! for audio playback. It handles audio output, volume control, seeking, and playback
//! state management for local audio files and streams.
//!
//! The `LocalPlayer` struct is the main entry point for local playback functionality,
//! managing audio output devices, playback state coordination, and progress tracking.

#![allow(clippy::module_name_repetitions)]

use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use atomic_float::AtomicF64;

use async_trait::async_trait;

use moosicbox_audio_decoder::{AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::{AudioHandle, AudioOutput, AudioOutputFactory};
use moosicbox_music_api::models::TrackAudioQuality;
use moosicbox_music_models::TrackApiSource;
use moosicbox_session::models::UpdateSession;
use switchy_async::util::CancellationToken;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};

use crate::{
    ApiPlaybackStatus, Playback, PlaybackHandler, PlaybackType, Player, PlayerError, PlayerSource,
    send_playback_event, symphonia::play_media_source, track_or_id_to_playable,
};

#[derive(Debug, Clone)]
struct ProgressUpdate {
    current_position: f64,
    session_id: u64,
    profile: String,
    playback_target: moosicbox_session::models::PlaybackTarget,
}

/// Local audio player implementation using Symphonia decoder.
///
/// This player handles local audio playback with support for various formats,
/// volume control, seeking, and pause/resume functionality. It manages audio
/// output through the `AudioOutputFactory` and coordinates playback state
/// across threads using shared atomics and channels.
#[derive(Clone)]
pub struct LocalPlayer {
    /// Unique identifier for this player instance
    pub id: u64,
    playback_type: PlaybackType,
    source: PlayerSource,
    /// Audio output factory for creating audio streams
    pub output: Option<Arc<Mutex<AudioOutputFactory>>>,

    /// Current playback session state
    pub playback: Arc<RwLock<Option<Playback>>>,
    /// Playback handler for this player
    pub playback_handler: Arc<RwLock<Option<PlaybackHandler>>>,
    /// Shared volume for immediate audio output updates
    pub shared_volume: Arc<AtomicF64>,
    /// Handle for immediate audio control
    pub audio_handle: Arc<RwLock<Option<AudioHandle>>>,
    session_command_forwarder:
        Arc<RwLock<Option<flume::Sender<moosicbox_audio_output::CommandMessage>>>>,
    session_coordinator_handle: Arc<RwLock<Option<switchy_async::task::JoinHandle<()>>>>,
}

impl std::fmt::Debug for LocalPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalPlayer")
            .field("id", &self.id)
            .field("playback_type", &self.playback_type)
            .field("source", &self.source)
            .field("output", &self.output)
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

        // Cleanup old session coordinator before creating new one
        self.cleanup_session_coordinator().await;

        switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("player: Play media source", {
                let playback = self.playback.clone();
                let shared_volume = self.shared_volume.clone();
                let output = self.output.clone().unwrap();
                let audio_handle_storage = self.audio_handle.clone();
                let session_coordinator_handle_storage = self.session_coordinator_handle.clone();
                let player_self = self.clone();
                move || {
                    // CREATE AUDIO HANDLE AND SESSION COORDINATOR
                    let (session_command_sender, session_command_receiver) = flume::unbounded();
                    let handle = moosicbox_audio_output::AudioHandle::new(session_command_sender);

                    // STORE HANDLE IMMEDIATELY - available for pause() right now
                    *audio_handle_storage.write().unwrap() = Some(handle);
                    log::debug!("trigger_play: created and stored audio handle immediately");

                    // START INSTANCE SESSION COMMAND COORDINATOR (no CPAL streams involved)
                    let coordinator_handle =
                        player_self.start_instance_session_coordinator(session_command_receiver);
                    *session_coordinator_handle_storage.write().unwrap() = Some(coordinator_handle);
                    log::debug!("trigger_play: started instance session command coordinator");

                    let mut handler = get_audio_decode_handler_with_command_receiver(
                        &playback,
                        shared_volume,
                        output,
                        seek,
                        player_self.clone(),
                    )?;

                    play_media_source(
                        mss,
                        &playable_track.hint,
                        &mut handler,
                        true,
                        true,
                        None,
                        seek,
                    )
                    .map_err(PlayerError::PlaybackError)
                }
            })
            .await??;

        log::info!("Finished playback for track_id={track_id}");

        // Assert that track playback finished close to the expected duration
        // This helps catch cases where tracks are being truncated early
        let playback_progress = {
            self.playback
                .read()
                .unwrap()
                .as_ref()
                .map_or(0.0, |p| p.progress)
        };

        let expected_duration = track.duration;
        let duration_tolerance = 1.0; // Allow 1 second tolerance

        log::debug!(
            "Playback completion check: track_id={track_id}, expected_duration={expected_duration:.2}s, actual_progress={playback_progress:.2}s, difference={difference:.2}s",
            difference = expected_duration - playback_progress
        );

        moosicbox_assert::assert!(
            playback_progress >= (expected_duration - duration_tolerance),
            "Track playback finished prematurely! Expected duration: {expected_duration:.2}s, Actual position: {playback_progress:.2}s, Difference: {difference:.2}s. Track: {track_id} '{title}'",
            difference = expected_duration - playback_progress,
            title = track.title
        );

        Ok(())
    }

    async fn trigger_stop(&self) -> Result<(), PlayerError> {
        log::info!("Stopping playback");

        // 1. Take ownership of the handle for immediate control and cleanup
        if let Some(handle) = self.take_current_audio_handle() {
            handle.reset().await?;
            log::debug!("Audio output reset successfully via handle");
            // Handle is automatically dropped here, ensuring cleanup
        } else {
            log::warn!("No audio output handle available to stop");
        }

        // 2. Cancel the decode/playback task (existing logic)
        {
            let Some(playback) = self.playback.read().unwrap().clone() else {
                return Err(PlayerError::NoPlayersPlaying);
            };

            log::debug!("Aborting playback {playback:?} for stop");
            playback.abort.cancel();
        }

        // Progress tracking is now handled by AudioOutput implementations

        self.playback.write().unwrap().as_mut().unwrap().abort = CancellationToken::new();

        Ok(())
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError> {
        log::info!("Pausing playback");

        // 1. Take ownership of the handle to pause and cleanup
        if let Some(handle) = self.take_current_audio_handle() {
            handle.pause().await?;
            log::debug!("Audio output paused successfully via handle");
            // Handle is automatically dropped here, ensuring cleanup
        } else {
            log::warn!("No audio output handle available to pause");
        }

        // 2. Cancel the decode/playback task (existing logic)
        {
            let Some(playback) = self.playback.read().unwrap().clone() else {
                return Err(PlayerError::NoPlayersPlaying);
            };

            let id = playback.id;

            log::info!("Aborting playback id {id} for pause");
            playback.abort.cancel();

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

        log::info!("Resuming playback from position: {progress:.2}s");

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
    /// Creates a new local player instance.
    ///
    /// Initializes a local player with the specified source and playback type,
    /// setting up audio output infrastructure and playback state management.
    ///
    /// # Errors
    ///
    /// * If failed to generate a `LocalPlayer` `id`
    pub async fn new(
        source: PlayerSource,
        playback_type: Option<PlaybackType>,
    ) -> Result<Self, PlayerError> {
        let id = switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("player: local player rng", || {
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

            playback_handler: Arc::new(RwLock::new(None)),
            shared_volume,
            audio_handle: Arc::new(RwLock::new(None)),
            session_command_forwarder: Arc::new(RwLock::new(None)),
            session_coordinator_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Sets the audio output factory for this player.
    #[must_use]
    pub fn with_output(mut self, output: AudioOutputFactory) -> Self {
        self.output.replace(Arc::new(Mutex::new(output)));
        self
    }

    /// Takes ownership of the current audio handle.
    ///
    /// Returns the audio handle if one exists, leaving `None` in its place.
    #[must_use]
    fn take_current_audio_handle(&self) -> Option<AudioHandle> {
        self.audio_handle.write().unwrap().take()
    }

    /// Cleans up the old session coordinator and forwarder before creating new ones.
    ///
    /// This aborts the existing coordinator task and clears the command forwarder.
    async fn cleanup_session_coordinator(&self) {
        // Take the handle outside the lock to avoid holding it across await
        let handle = self.session_coordinator_handle.write().unwrap().take();

        if let Some(handle) = handle {
            log::debug!("Aborting old session coordinator");
            handle.abort();
            // Wait for it to finish
            let _ = handle.await;
        }

        // Clear old forwarder
        *self.session_command_forwarder.write().unwrap() = None;
        log::debug!("Cleaned up old session coordinator and forwarder");
    }

    /// Starts an instance-level session command coordinator that forwards commands to thread-local processors.
    /// This ensures only one active processor per `LocalPlayer` instance while maintaining macOS CPAL compatibility.
    fn start_instance_session_coordinator(
        &self,
        session_command_receiver: flume::Receiver<moosicbox_audio_output::CommandMessage>,
    ) -> switchy_async::task::JoinHandle<()> {
        let session_command_forwarder = self.session_command_forwarder.clone();
        switchy_async::runtime::Handle::current().spawn_with_name("instance_session_coordinator", async move {
            log::debug!("Instance session command coordinator started");

            while let Ok(command_msg) = session_command_receiver.recv_async().await {
                log::trace!(
                    "Coordinating instance session command: {:?}",
                    command_msg.command
                );

                // Forward command to the currently registered thread-local processor for this instance
                let forwarder = session_command_forwarder.read().unwrap().clone();

                if let Some(forwarder) = forwarder {
                    // Forward the command to the thread-local processor
                    if let Err(e) = forwarder.send_async(command_msg).await {
                        log::warn!("Failed to forward command to thread-local processor: {e}");
                    }
                } else {
                    log::warn!(
                        "No thread-local processor registered for instance session command: {:?}",
                        command_msg.command
                    );
                    // Send error response if requested
                    if let Some(response_sender) = command_msg.response_sender {
                        let _ = response_sender
                            .send_async(moosicbox_audio_output::AudioResponse::Error(
                                "No audio processor available".to_string(),
                            ))
                            .await;
                    }
                }
            }

            log::debug!("Instance session command coordinator stopped");
        })
    }

    /// Registers a thread-local processor with the session coordinator.
    /// The processor stays on its original thread (macOS CPAL compatible).
    fn register_thread_local_processor(&self, audio_output_handle: AudioHandle) {
        let (thread_local_sender, thread_local_receiver) = flume::unbounded();

        // Replace the instance forwarder with this thread-local processor
        *self.session_command_forwarder.write().unwrap() = Some(thread_local_sender);
        log::debug!("Registered thread-local processor with session coordinator");

        // Start the thread-local processor (stays on current thread)
        switchy_async::runtime::Handle::current().spawn_with_name(
            "thread_local_audio_processor",
            async move {
                log::debug!("Thread-local audio processor started");

                while let Ok(command_msg) = thread_local_receiver.recv_async().await {
                    log::trace!(
                        "Processing thread-local audio command: {:?}",
                        command_msg.command
                    );

                    let response = match command_msg.command {
                        moosicbox_audio_output::AudioCommand::Pause => {
                            match audio_output_handle.pause().await {
                                Ok(()) => moosicbox_audio_output::AudioResponse::Success,
                                Err(e) => moosicbox_audio_output::AudioResponse::Error(format!(
                                    "Failed to pause: {e}"
                                )),
                            }
                        }
                        moosicbox_audio_output::AudioCommand::Resume => {
                            match audio_output_handle.resume().await {
                                Ok(()) => moosicbox_audio_output::AudioResponse::Success,
                                Err(e) => moosicbox_audio_output::AudioResponse::Error(format!(
                                    "Failed to resume: {e}"
                                )),
                            }
                        }
                        moosicbox_audio_output::AudioCommand::SetVolume(volume) => {
                            match audio_output_handle.set_volume(volume).await {
                                Ok(()) => moosicbox_audio_output::AudioResponse::Success,
                                Err(e) => moosicbox_audio_output::AudioResponse::Error(format!(
                                    "Failed to set volume: {e}"
                                )),
                            }
                        }
                        moosicbox_audio_output::AudioCommand::Reset => {
                            match audio_output_handle.reset().await {
                                Ok(()) => moosicbox_audio_output::AudioResponse::Success,
                                Err(e) => moosicbox_audio_output::AudioResponse::Error(format!(
                                    "Failed to reset: {e}"
                                )),
                            }
                        }
                        _ => moosicbox_audio_output::AudioResponse::Error(
                            "Command not supported".to_string(),
                        ),
                    };

                    // Send response if requested
                    if let Some(response_sender) = command_msg.response_sender {
                        let _ = response_sender.send_async(response).await;
                    }
                }

                log::debug!("Thread-local audio processor stopped");
            },
        );
    }
}

#[allow(clippy::too_many_lines)]
fn get_audio_decode_handler_with_command_receiver(
    playback: &Arc<RwLock<Option<Playback>>>,
    shared_volume: Arc<AtomicF64>,
    output: Arc<Mutex<AudioOutputFactory>>,
    seek: Option<f64>,
    player: LocalPlayer,
) -> Result<AudioDecodeHandler, PlayerError> {
    // Initialize shared volume with the current playback volume
    let initial_volume = {
        playback.read().unwrap().as_ref().map_or(1.0, |playback| {
            playback.volume.load(std::sync::atomic::Ordering::SeqCst)
        })
    };

    shared_volume.store(initial_volume, std::sync::atomic::Ordering::SeqCst);
    log::info!(
        "LocalPlayer: initialized shared volume to {initial_volume:.3} (from current playback)"
    );

    let sent_playback_start_event = AtomicBool::new(false);

    let mut audio_decode_handler = AudioDecodeHandler::new()
        .with_filter(Box::new({
            let playback = playback.clone();
            let initial_seek_position = seek.unwrap_or(0.0);
            move |_decoded, _packet, _track| {
                // Just send the initial playback start event, don't track progress here
                if !sent_playback_start_event.load(std::sync::atomic::Ordering::SeqCst) {
                    let binding = playback.read().unwrap();
                    if let Some(playback) = binding.as_ref()
                        && let Some(playback_target) = playback.playback_target.clone() {
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
                Ok(())
            }
        }))
        .with_output(Box::new({
            let seek_position = seek.unwrap_or(0.0);
            let shared_volume_local = shared_volume;
            let playback_for_callback = playback.clone();
            move |spec, _duration| {
                use moosicbox_audio_output::AudioWrite;

                let mut output: AudioOutput = (output.lock().unwrap())
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

                // REGISTER THREAD-LOCAL PROCESSOR with instance session coordinator
                player.register_thread_local_processor(output.handle());
                log::debug!("Audio output creation: registered thread-local processor");

                // Set up progress callback to handle progress events from AudioOutput
                // Create a channel for progress updates to avoid calling async code from audio thread
                let (progress_tx, progress_rx) = flume::unbounded::<ProgressUpdate>();

                // Spawn a task to handle progress updates from the audio thread
                let playback_for_handler = playback_for_callback.clone();
                switchy_async::runtime::Handle::current().spawn_with_name("player: Progress handler", async move {
                    let mut last_reported_second: Option<u64> = None;

                    while let Ok(progress_update) = progress_rx.recv_async().await {
                        let old = {
                            let mut binding = playback_for_handler.write().unwrap();
                            if let Some(playback) = binding.as_mut() {
                                let old = playback.clone();
                                playback.progress = progress_update.current_position;
                                Some(old)
                            } else {
                                log::warn!("Progress handler: no playback available to update");
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
                        } else {
                            log::warn!("Progress callback: no playback info available");
                        }
                    })
                };

                output.set_progress_callback(Some(progress_callback));
                log::debug!("Audio output creation: set progress callback");

                Ok(Box::new(output))
            }
        }));

    if let Some(playback) = playback.read().unwrap().as_ref() {
        audio_decode_handler = audio_decode_handler.with_cancellation_token(playback.abort.clone());
    }

    moosicbox_assert::assert_or_err!(
        audio_decode_handler.contains_outputs_to_open(),
        crate::symphonia::PlaybackError::NoAudioOutputs.into(),
        "No outputs set for the audio_decode_handler"
    );

    Ok(audio_decode_handler)
}
