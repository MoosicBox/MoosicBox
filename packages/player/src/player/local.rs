use std::sync::{atomic::AtomicBool, Arc, RwLock, RwLockWriteGuard};

use async_trait::async_trait;
use atomic_float::AtomicF64;
use crossbeam_channel::Receiver;
use moosicbox_core::{
    sqlite::models::{ApiSource, ToApi, TrackApiSource, UpdateSession},
    types::PlaybackQuality,
};
use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use moosicbox_symphonia_player::{
    media_sources::remote_bytestream::RemoteByteStreamMediaSource, output::AudioOutputHandler,
    volume_mixer::mix_volume,
};
use rand::{thread_rng, Rng as _};
use symphonia::core::{io::MediaSourceStream, probe::Hint};
use tokio_util::sync::CancellationToken;

use crate::player::{
    get_track_url, send_playback_event, trigger_playback_event, ApiPlaybackStatus, PlayableTrack,
    Playback, PlaybackRetryOptions, PlaybackType, Player, PlayerError, PlayerSource, TrackOrId,
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
    fn active_playback_write(&self) -> RwLockWriteGuard<'_, Option<Playback>> {
        self.active_playback.write().unwrap()
    }

    fn receiver_write(&self) -> RwLockWriteGuard<'_, Option<Receiver<()>>> {
        self.receiver.write().unwrap()
    }

    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError> {
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

        let response: Result<i32, PlayerError> = tokio::task::spawn_blocking(move || {
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

            Ok(moosicbox_symphonia_player::play_media_source(
                mss,
                &playable_track.hint,
                &mut audio_output_handler,
                true,
                true,
                None,
                seek,
            )?)
        })
        .await?;

        if let Err(e) = response {
            log::error!("Failed to play playback: {e:?}");
            return Err(PlayerError::NoPlayersPlaying);
        }

        log::info!("Finished playback for track_id={}", track_id);

        Ok(())
    }

    async fn trigger_stop(&self) -> Result<Playback, PlayerError> {
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
    ) -> Result<(), PlayerError> {
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
            ",
            self.source
        );

        if stop.unwrap_or(false) {
            self.stop(retry_options).await?;
        }

        let mut should_play = modify_playback && play.unwrap_or(false);

        let playback = if let Ok(playback) = self.get_playback() {
            log::trace!("update_playback: existing playback={playback:?}");
            if playback.playing {
                if let Some(false) = playing {
                    self.pause(retry_options).await?;
                }
            } else {
                should_play = modify_playback && (should_play || playing.unwrap_or(false));
            }

            playback
        } else {
            log::trace!("update_playback: no existing playback");
            should_play = modify_playback && (should_play || playing.unwrap_or(false));

            Playback::new(
                tracks.clone().unwrap_or_default(),
                position,
                AtomicF64::new(volume.unwrap_or(1.0)),
                quality.unwrap_or_default(),
                session_id,
                session_playlist_id,
            )
        };

        log::debug!("update_playback: modify_playback={modify_playback} should_play={should_play}");

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

            Ok(())
        }
    }

    async fn trigger_pause(&self) -> Result<(), PlayerError> {
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

        Ok(())
    }

    async fn trigger_resume(&self) -> Result<(), PlayerError> {
        let mut playback = self.get_playback()?;

        let id = playback.id;

        if playback.playing {
            return Err(PlayerError::PlaybackAlreadyPlaying(id));
        }

        let seek = Some(playback.progress);

        playback.playing = true;
        playback.abort = CancellationToken::new();

        self.trigger_play(seek).await
    }

    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError> {
        self.trigger_stop().await?;
        self.trigger_play(Some(seek)).await?;
        Ok(())
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
