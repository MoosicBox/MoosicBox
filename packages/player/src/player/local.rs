use std::sync::{atomic::AtomicBool, Arc, RwLock};

use async_trait::async_trait;
use atomic_float::AtomicF64;
use crossbeam_channel::{bounded, Receiver};
use moosicbox_core::{
    sqlite::models::{ApiSource, ToApi, TrackApiSource, UpdateSession},
    types::PlaybackQuality,
};
use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStreamMediaSource,
    output::{AudioOutputError, AudioOutputHandler},
    volume_mixer::mix_volume,
    PlaybackError,
};
use rand::{thread_rng, Rng as _};
use symphonia::core::{io::MediaSourceStream, probe::Hint};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::player::{
    get_track_url, send_playback_event, trigger_playback_event, ApiPlaybackStatus, PlayableTrack,
    Playback, PlaybackRetryOptions, PlaybackStatus, PlaybackType, Player, PlayerError,
    PlayerSource, TrackOrId, RT,
};

#[derive(Clone)]
pub struct LocalPlayer {
    pub id: usize,
    playback_type: PlaybackType,
    source: PlayerSource,
    pub active_playback: Arc<RwLock<Option<Playback>>>,
    receiver: Arc<RwLock<Option<Receiver<()>>>>,
}

#[async_trait]
impl Player for LocalPlayer {
    async fn play_playback(
        &self,
        mut playback: Playback,
        seek: Option<f64>,
        retry_options: Option<PlaybackRetryOptions>,
    ) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Playing playback...");
        if let Ok(playback) = self.get_playback() {
            log::debug!("Stopping existing playback {}", playback.id);
            self.stop().await?;
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
            log::info!(
                "Playing track with Symphonia: {} {abort:?} {track_or_id:?}",
                track_id
            );

            let playback_type = match track_or_id.track_source() {
                TrackApiSource::Local => self.playback_type,
                _ => PlaybackType::Stream,
            };

            let playable_track = self
                .track_or_id_to_playable(playback_type, &track_or_id, quality)
                .await?;
            let mss = MediaSourceStream::new(playable_track.source, Default::default());

            let active_playback = self.active_playback.clone();
            let sent_playback_start_event = AtomicBool::new(false);

            let retry = tokio::task::spawn_blocking(move || {
                let mut audio_output_handler = AudioOutputHandler::new()
                    .with_filter(Box::new({
                        let active_playback = active_playback.clone();
                        move |_decoded, packet, track| {
                            if let Some(tb) = track.codec_params.time_base {
                                let ts = packet.ts();
                                let t = tb.calc_time(ts);
                                let secs = f64::from(t.seconds as u32) + t.frac;

                                let mut binding = active_playback.write().unwrap();
                                if let Some(playback) = binding.as_mut() {
                                    if !sent_playback_start_event.load(std::sync::atomic::Ordering::SeqCst)
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
                                                seek: Some(secs),
                                                volume: None,
                                                playlist: None,
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
                        mix_volume(decoded, volume.load(std::sync::atomic::Ordering::SeqCst));
                        Ok(())
                    }))
                    .with_cancellation_token(abort.clone());

                #[cfg(feature = "cpal")]
                {
                    audio_output_handler = audio_output_handler.with_output(Box::new(
                        moosicbox_symphonia_player::output::cpal::player::try_open,
                    ));
                }
                #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
                {
                    audio_output_handler = audio_output_handler.with_output(Box::new(
                        moosicbox_symphonia_player::output::pulseaudio::simple::try_open,
                    ));
                }
                #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
                {
                    audio_output_handler = audio_output_handler.with_output(Box::new(
                        moosicbox_symphonia_player::output::pulseaudio::standard::try_open,
                    ));
                }

                moosicbox_assert::assert_or_err!(
                    audio_output_handler.contains_outputs_to_open(),
                    PlayerError::NoAudioOutputs,
                    "No outputs set for the audio_output_handler"
                );

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
                        log::error!("Track playback failed and no retry options: {err:?}");
                        return Err(PlayerError::PlaybackError(err));
                    }

                    let retry_options = retry_options.unwrap();

                    if let PlaybackError::AudioOutput(AudioOutputError::Interrupt) = err {
                        retry_count += 1;
                        if retry_count > retry_options.max_retry_count {
                            log::error!(
                                "Playback retry failed after {retry_count} attempts. Not retrying"
                            );
                            return Ok(false);
                        }
                        let binding = active_playback.read().unwrap();
                        let playback = binding.as_ref().unwrap();
                        current_seek = Some(playback.progress);
                        log::warn!("Playback interrupted. Trying again at position {current_seek:?} (attempt {retry_count}/{})", retry_options.max_retry_count);
                        return Ok(true);
                    }
                }
                Ok(false)
            }).await??;

            if retry {
                continue;
            }

            log::info!("Finished playback for track {}", track_id);
            break;
        }

        Ok(())
    }

    async fn stop(&self) -> Result<Playback, PlayerError> {
        log::info!("Stopping playback");
        let playback = self.get_playback()?;

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
            source={:?}\n\t\
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
            ",
            self.source
        );

        if stop.unwrap_or(false) {
            return Ok(PlaybackStatus {
                success: true,
                playback_id: self.stop().await?.id,
            });
        }

        let mut should_play = play.unwrap_or(false);

        let playback = if let Ok(playback) = self.get_playback() {
            log::trace!("update_playback: existing playback={playback:?}");
            if playback.playing {
                if let Some(false) = playing {
                    self.pause_playback().await?;
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
                session_playlist_id,
            )
        };

        log::debug!("update_playback: should_play={should_play}");

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
        let seek = if playback.progress != 0.0 {
            Some(playback.progress)
        } else {
            None
        };

        if should_play {
            self.play_playback(playback, seek, retry_options).await
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

    async fn pause_playback(&self) -> Result<PlaybackStatus, PlayerError> {
        log::info!("Pausing playback id");
        let mut playback = self.get_playback()?;

        let id = playback.id;

        log::info!("Aborting playback id {id} for pause");
        playback.abort.cancel();

        if !playback.playing {
            return Err(PlayerError::PlaybackNotPlaying(id));
        }

        log::trace!("Waiting for playback completion response");
        if let Some(receiver) = self.receiver.write().unwrap().take() {
            if let Err(err) = receiver.recv() {
                log::error!("Sender correlated with receiver has dropped: {err:?}");
            }
        } else {
            log::debug!("No receiver to wait for completion response with");
        }
        log::trace!("Playback successfully stopped");

        playback.playing = false;
        playback.abort = CancellationToken::new();

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
            false,
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
                quality.format == moosicbox_core::types::AudioFormat::Flac
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
        if let Some(playback) = self.active_playback.read().unwrap().clone() {
            Ok(playback.clone())
        } else {
            Err(PlayerError::NoPlayersPlaying)
        }
    }
}

impl LocalPlayer {
    pub fn new(source: PlayerSource, playback_type: Option<PlaybackType>) -> LocalPlayer {
        LocalPlayer {
            id: thread_rng().gen::<usize>(),
            playback_type: playback_type.unwrap_or_default(),
            source,
            active_playback: Arc::new(RwLock::new(None)),
            receiver: Arc::new(RwLock::new(None)),
        }
    }
}
