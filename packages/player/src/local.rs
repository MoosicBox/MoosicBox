#![allow(clippy::module_name_repetitions)]

use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock};

use async_trait::async_trait;
use flume::Receiver;
use moosicbox_audio_decoder::{AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::{AudioOutput, AudioOutputFactory};
use moosicbox_music_models::TrackApiSource;
use moosicbox_session::models::UpdateSession;
use rand::{rng, Rng as _};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use tokio_util::sync::CancellationToken;

use crate::{
    send_playback_event, symphonia::play_media_source_async, track_or_id_to_playable,
    trigger_playback_event, volume_mixer::mix_volume, ApiPlaybackStatus, Playback, PlaybackHandler,
    PlaybackType, Player, PlayerError, PlayerSource,
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
            &self.source,
            playback.abort.clone(),
        )
        .await?;
        let mss =
            MediaSourceStream::new(playable_track.source, MediaSourceStreamOptions::default());

        let active_playback = self.playback.clone();
        let sent_playback_start_event = AtomicBool::new(false);

        if self.output.is_none() {
            return Err(PlayerError::NoAudioOutputs);
        }

        let open_func = self.output.clone().unwrap();

        let get_handler = move || {
            #[allow(unused_mut)]
            let mut audio_decode_handler = AudioDecodeHandler::new()
                .with_filter(Box::new({
                    let active_playback = active_playback.clone();
                    move |_decoded, packet, track| {
                        if let Some(tb) = track.codec_params.time_base {
                            let ts = packet.ts();
                            let t = tb.calc_time(ts);
                            let secs = f64::from(u32::try_from(t.seconds).unwrap()) + t.frac;

                            let mut binding = active_playback.write().unwrap();
                            if let Some(playback) = binding.as_mut() {
                                if !sent_playback_start_event
                                    .load(std::sync::atomic::Ordering::SeqCst)
                                {
                                    if let Some(playback_target) = playback.playback_target.clone() {
                                        sent_playback_start_event
                                            .store(true, std::sync::atomic::Ordering::SeqCst);

                                        log::debug!(
                                            "trigger_play: Sending initial progress event seek={secs}"
                                        );

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
                                            seek: Some(secs),
                                            volume: None,
                                            playlist: None,
                                            quality: None,
                                        };
                                        send_playback_event(&update, playback);
                                    }
                                }

                                let old = playback.clone();
                                playback.progress = secs;
                                trigger_playback_event(playback, &old);
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
                .with_output(Box::new(move |_spec, _duration| {
                    let output: AudioOutput = (open_func.lock().unwrap())
                        .try_into_output()
                        .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;

                    Ok(Box::new(output))
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

        self.playback.write().unwrap().as_mut().unwrap().abort = CancellationToken::new();

        Ok(())
    }

    async fn trigger_resume(&self) -> Result<(), PlayerError> {
        let progress = {
            self.playback
                .read()
                .unwrap()
                .as_ref()
                .ok_or(PlayerError::NoPlayersPlaying)?
                .progress
        };

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
                rng().random::<u64>()
            })
            .await?,
            playback_type: playback_type.unwrap_or_default(),
            source,
            output: None,
            playback: Arc::new(RwLock::new(None)),
            receiver: Arc::new(tokio::sync::RwLock::new(None)),
            playback_handler: Arc::new(RwLock::new(None)),
        })
    }

    #[must_use]
    pub fn with_output(mut self, output: AudioOutputFactory) -> Self {
        self.output.replace(Arc::new(Mutex::new(output)));
        self
    }
}
