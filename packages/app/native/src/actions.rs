use hyperchad::{
    actions::{self, logic::Value},
    renderer::PartialView,
};
use moosicbox_app_native_ui::{Action, albums::load_albums};
use moosicbox_app_state::AppStateError;
use moosicbox_music_api::{SourceToMusicApi as _, profiles::PROFILES};
use moosicbox_session_models::{UpdateSession, UpdateSessionPlaylist};
use moosicbox_ws::models::{InboundPayload, UpdateSessionPayload};

use crate::{PROFILE, RENDERER, STATE};

/// # Errors
///
/// If an error occurs relating to the `AppState`
///
/// # Panics
///
/// * If the default `PROFILE` is missing.
#[allow(clippy::too_many_lines)]
pub async fn handle_action(action: Action, value: Option<Value>) -> Result<(), AppStateError> {
    log::debug!("handle_action: action={action:?} value={value:?}");

    match &action {
        Action::RefreshVisualization => {
            #[cfg(feature = "_canvas")]
            {
                static EPSILON: f32 = 0.001;

                log::debug!("handle_action: RefreshVisualization: {value:?}");

                let width = value
                    .ok_or(AppStateError::ActionMissingParam)?
                    .as_f32(None::<&Box<dyn Fn(&actions::logic::CalcValue) -> Option<Value>>>)
                    .ok_or(AppStateError::ActionInvalidParam)?;

                let height = moosicbox_app_native_ui::VIZ_HEIGHT;
                let height = f32::from(height);

                let (ew, eh) = crate::visualization::get_dimensions();

                if (ew - width).abs() >= EPSILON || (eh - height).abs() >= EPSILON {
                    log::debug!(
                        "handle_action: updating visualization width={width} height={height}"
                    );
                    crate::visualization::set_dimensions(width, height);
                }

                crate::visualization::check_visualization_update().await;
            }

            Ok(())
        }
        Action::TogglePlayback
        | Action::PreviousTrack
        | Action::NextTrack
        | Action::SetVolume
        | Action::SeekCurrentTrackPercent
        | Action::PlayAlbum { .. }
        | Action::AddAlbumToQueue { .. }
        | Action::PlayAlbumStartingAtTrackId { .. }
        | Action::PlayTracks { .. } => {
            let Some(session) = STATE.get_current_session().await else {
                log::debug!("handle_action: no current session");
                return Ok(());
            };
            let Some(profile) = STATE.profile.read().await.clone() else {
                log::debug!("handle_action: no current session");
                return Ok(());
            };

            let playback_target =
                { STATE.current_playback_target.read().await.clone() }.or(session.playback_target);

            let Some(playback_target) = playback_target else {
                log::debug!("handle_action: no playback_target");
                return Ok(());
            };

            match &action {
                Action::RefreshVisualization | Action::FilterAlbums { .. } => unreachable!(),
                Action::TogglePlayback => {
                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: None,
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: Some(!session.playing),
                                    position: None,
                                    seek: None,
                                    volume: None,
                                    playlist: None,
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
                Action::PreviousTrack => {
                    if let Some(position) = session.position {
                        let seek = session.seek.unwrap_or(0.0);
                        let position = if seek < 5.0 && position > 0 {
                            position - 1
                        } else {
                            position
                        };

                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: Some(position),
                                        seek: Some(0.0),
                                        volume: None,
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    } else {
                        Ok(())
                    }
                }
                Action::NextTrack => {
                    if let Some(position) = session.position {
                        if usize::from(position) + 1 >= session.playlist.tracks.len() {
                            log::debug!("handle_action: already at last track");
                            return Ok(());
                        }
                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: Some(position + 1),
                                        seek: Some(0.0),
                                        volume: None,
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    } else {
                        Ok(())
                    }
                }
                Action::SetVolume => {
                    log::debug!("handle_action: SetVolume: {value:?}");
                    let volume = value
                        .ok_or(AppStateError::ActionMissingParam)?
                        .as_f32(None::<&Box<dyn Fn(&actions::logic::CalcValue) -> Option<Value>>>)
                        .ok_or(AppStateError::ActionInvalidParam)?;
                    if STATE.get_current_session().await.is_some_and(|x| {
                        x.volume
                            .is_some_and(|x| (x - f64::from(volume)).abs() < 0.01)
                    }) {
                        log::debug!("handle_action: SetVolume: already at desired volume");
                        Ok(())
                    } else {
                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: None,
                                        seek: None,
                                        volume: Some(f64::from(volume)),
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    }
                }
                Action::SeekCurrentTrackPercent => {
                    log::debug!("handle_action: SeekCurrentTrackPercent: {value:?}");
                    let seek = value
                        .ok_or(AppStateError::ActionMissingParam)?
                        .as_f32(None::<&Box<dyn Fn(&actions::logic::CalcValue) -> Option<Value>>>)
                        .ok_or(AppStateError::ActionInvalidParam)?;
                    let session = STATE.get_current_session_ref().await;
                    if let Some(session) = session {
                        if let Some(position) = session.position {
                            if let Some(duration) = session
                                .playlist
                                .tracks
                                .get(position as usize)
                                .map(|x| x.duration)
                            {
                                let seek = duration * f64::from(seek);

                                if seek < 0.0 || seek > duration {
                                    log::debug!(
                                        "handle_action: SeekCurrentTrackPercent: target seek is out of track duration bounds"
                                    );
                                    Ok(())
                                } else if session.seek.is_some_and(|x| (x - seek).abs() < 0.1) {
                                    log::debug!(
                                        "handle_action: SeekCurrentTrackPercent: already at desired position"
                                    );
                                    Ok(())
                                } else {
                                    STATE
                                        .queue_ws_message(
                                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                                payload: UpdateSession {
                                                    session_id: session.session_id,
                                                    profile,
                                                    playback_target,
                                                    play: None,
                                                    stop: None,
                                                    name: None,
                                                    active: None,
                                                    playing: None,
                                                    position: None,
                                                    seek: Some(seek),
                                                    volume: None,
                                                    playlist: None,
                                                    quality: None,
                                                },
                                            }),
                                            true,
                                        )
                                        .await
                                }
                            } else {
                                log::debug!("handle_action: SeekCurrentTrackPercent: no track");
                                Ok(())
                            }
                        } else {
                            log::debug!("handle_action: SeekCurrentTrackPercent: no position");
                            Ok(())
                        }
                    } else {
                        log::debug!("handle_action: SeekCurrentTrackPercent: no session");
                        Ok(())
                    }
                }
                Action::PlayAlbum {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                }
                | Action::PlayAlbumStartingAtTrackId {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                    ..
                }
                | Action::AddAlbumToQueue {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                } => {
                    let api = PROFILES
                        .get(PROFILE)
                        .unwrap()
                        .get(api_source)
                        .ok_or_else(|| AppStateError::unknown("Invalid source"))?;
                    let versions = api
                        .album_versions(album_id, None, None)
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))?
                        .clone();
                    let Some(version) = versions
                        .iter()
                        .find(|x| {
                            version_source.as_ref().is_none_or(|y| &x.source == y)
                                && sample_rate.is_none_or(|y| x.sample_rate.is_some_and(|x| x == y))
                                && bit_depth.is_none_or(|y| x.bit_depth.is_some_and(|x| x == y))
                        })
                        .or_else(|| versions.first())
                        .cloned()
                    else {
                        log::debug!("handle_action: no album tracks");
                        return Ok(());
                    };

                    let play = matches!(
                        action,
                        Action::PlayAlbum { .. } | Action::PlayAlbumStartingAtTrackId { .. }
                    );

                    let tracks =
                        if let Action::PlayAlbumStartingAtTrackId { start_track_id, .. } = action {
                            if let Some(index) =
                                version.tracks.iter().position(|x| x.id == start_track_id)
                            {
                                version.tracks.into_iter().skip(index).collect()
                            } else {
                                vec![]
                            }
                        } else {
                            version.tracks
                        };

                    let mut tracks = tracks.into_iter().map(Into::into).collect();

                    if !play {
                        tracks = [session.playlist.tracks, tracks].concat();
                    }

                    let position = if play { Some(0) } else { None };
                    let seek = if play { Some(0.0) } else { None };

                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: Some(play),
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: None,
                                    position,
                                    seek,
                                    volume: None,
                                    playlist: Some(UpdateSessionPlaylist {
                                        session_playlist_id: session.playlist.session_playlist_id,
                                        tracks,
                                    }),
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
                Action::PlayTracks {
                    track_ids,
                    api_source,
                } => {
                    let api = PROFILES
                        .get(PROFILE)
                        .unwrap()
                        .get(api_source)
                        .ok_or_else(|| AppStateError::unknown("Invalid source"))?;
                    let tracks = api
                        .tracks(Some(track_ids), None, None, None, None)
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))?
                        .map(Into::into)
                        .items()
                        .to_vec();

                    let position = Some(0);
                    let seek = Some(0.0);

                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: Some(true),
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: None,
                                    position,
                                    seek,
                                    volume: None,
                                    playlist: Some(UpdateSessionPlaylist {
                                        session_playlist_id: session.playlist.session_playlist_id,
                                        tracks,
                                    }),
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
            }
        }
        Action::FilterAlbums {
            filtered_sources,
            sort,
        } => {
            let value = value.ok_or(AppStateError::ActionMissingParam)?;
            let filter = value.as_str().ok_or(AppStateError::ActionInvalidParam)?;
            log::debug!("handle_action: FilterAlbums filter={filter}");

            let size: u16 = 200;

            let view = PartialView {
                target: "albums".to_string(),
                container: load_albums(size, *sort, filtered_sources, filter)
                    .try_into()
                    .unwrap(),
            };
            let response = RENDERER.get().unwrap().render_partial(view).await;
            if let Err(e) = response {
                log::error!("Failed to render_partial: {e:?}");
            }

            Ok(())
        }
    }
}
