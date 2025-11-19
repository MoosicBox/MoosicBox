//! Event handlers for application state changes and playback updates.
//!
//! This module contains event listeners that respond to changes in the application state,
//! such as playback session updates, audio zone changes, and server connection updates.
//! These handlers update the UI in response to state changes and manage the current
//! playback session selection.

use hyperchad::renderer::View;
use moosicbox_app_native_ui::state::State;
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_music_models::api::ApiTrack;
use moosicbox_player::Playback;
use moosicbox_session_models::{
    ApiConnection, ApiPlaybackTarget, ApiSession, ApiUpdateSession, ApiUpdateSessionPlaylist,
    UpdateSession,
};

use crate::{RENDERER, STATE, convert_state};

/// Handles updates to the current playback sessions.
///
/// This function is called when the list of active playback sessions changes. It updates
/// the current session ID if a matching session exists, or selects the first available
/// session if no current session is set.
///
/// # Panics
///
/// * If fails to navigate to the home page on native platforms (egui, fltk)
pub async fn current_sessions_updated(sessions: Vec<ApiSession>) {
    log::trace!("current_sessions_updated: {sessions:?}");

    let session_id = *STATE.current_session_id.read().await;

    #[allow(clippy::collapsible_else_if)]
    if let Some(session_id) = session_id {
        if let Some(session) = sessions.into_iter().find(|x| x.session_id == session_id) {
            log::debug!("current_sessions_updated: setting current_session_id to matching session");
            set_current_session(session).await;
        } else {
            log::debug!(
                "current_sessions_updated: no matching session with session_id={session_id}"
            );
            STATE.current_session_id.write().await.take();
        }
    } else {
        if let Some(first) = sessions.into_iter().next() {
            log::debug!("current_sessions_updated: setting current_session_id to first session");
            set_current_session(first).await;
        } else {
            log::debug!("current_sessions_updated: no sessions");
            STATE.current_session_id.write().await.take();
        }
        #[cfg(any(feature = "egui", feature = "fltk"))]
        {
            log::debug!("app_native: navigating to home");
            crate::ROUTER
                .get()
                .unwrap()
                .navigate_spawn("/")
                .await
                .expect("Failed to navigate to home")
                .expect("Failed to navigate to home");
        }
    }
}

/// Handles updates to the connection list.
///
/// This function refreshes the audio zone and sessions display when the list of
/// server connections changes.
pub async fn connections_updated(_connections: Vec<ApiConnection>) {
    log::trace!("connections_updated");

    refresh_audio_zone_with_sessions().await;
}

/// Handles updates to audio zones and their associated sessions.
///
/// This function refreshes the audio zone and sessions display when audio zone
/// configuration or session assignments change.
pub async fn audio_zone_with_sessions_updated(_zones: Vec<ApiAudioZoneWithSession>) {
    log::trace!("audio_zone_with_sessions_updated");

    refresh_audio_zone_with_sessions().await;
}

async fn refresh_audio_zone_with_sessions() {
    log::trace!("refresh_audio_zone_with_sessions");

    let zones = STATE.current_audio_zones.read().await;
    let connections = STATE.current_connections.read().await;

    update_audio_zones(&zones, &connections).await;
}

async fn update_audio_zones(zones: &[ApiAudioZoneWithSession], connections: &[ApiConnection]) {
    let container = moosicbox_app_native_ui::audio_zones::audio_zones(zones, connections);
    let view = View::builder().with_fragment(container).build();
    let response = RENDERER.get().unwrap().render(view).await;
    if let Err(e) = response {
        log::error!("Failed to render: {e:?}");
    }
}

/// Handles playback session updates and refreshes the UI.
///
/// This function is called when a playback session is updated (e.g., play/pause, track change,
/// volume adjustment). It spawns a task to render the updated UI partials and updates the
/// visualization if the canvas feature is enabled.
pub async fn handle_playback_update(update: ApiUpdateSession) {
    moosicbox_logging::debug_or_trace!(
        ("handle_playback_update"),
        ("handle_playback_update: update={update:?}")
    );

    switchy_async::runtime::Handle::current().spawn_with_name(
        "moosicbox_app: handle_playback_update: render partials",
        async move {
            if let Some(session) = STATE.get_current_session().await {
                let state = convert_state(&STATE).await;

                handle_session_update(&state, &update, &session).await;
            } else {
                log::debug!("handle_playback_update: no session");
            }
        },
    );

    #[cfg(feature = "_canvas")]
    crate::visualization::check_visualization_update().await;
}

/// Playback event listener that handles session update events.
///
/// This function is registered as a callback with the playback system and is invoked
/// when playback events occur (e.g., play, pause, seek, track change). It spawns
/// an async task to handle the update asynchronously.
pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    log::debug!("on_playback_event: received update, spawning task to handle update={update:?}");

    switchy_async::runtime::Handle::current().spawn_with_name(
        "moosicbox_app: handle_playback_event",
        handle_playback_update(update.to_owned().into()),
    );
}

async fn set_current_session(session: ApiSession) {
    log::debug!("set_current_session: setting current session to session={session:?}");
    STATE
        .current_session_id
        .write()
        .await
        .replace(session.session_id);

    let update = ApiUpdateSession {
        session_id: session.session_id,
        profile: STATE.profile.read().await.clone().unwrap(),
        playback_target: ApiPlaybackTarget::AudioZone { audio_zone_id: 0 },
        play: None,
        stop: None,
        name: Some(session.name.clone()),
        active: Some(session.active),
        playing: Some(session.playing),
        position: session.position,
        seek: session.seek,
        volume: session.volume,
        playlist: Some(ApiUpdateSessionPlaylist {
            session_playlist_id: session.playlist.session_playlist_id,
            tracks: session.playlist.tracks.clone(),
        }),
        quality: None,
    };

    let state = convert_state(&STATE).await;

    handle_session_update(&state, &update, &session).await;

    #[cfg(feature = "_canvas")]
    crate::visualization::check_visualization_update().await;
}

async fn handle_session_update(state: &State, update: &ApiUpdateSession, session: &ApiSession) {
    let renderer = RENDERER.get().unwrap();

    let markup = moosicbox_app_native_ui::session_updated(state, update, session);
    let view = View::builder().with_fragments(markup).build();
    let response = renderer.render(view).await;
    if let Err(e) = response {
        log::error!("Failed to render: {e:?}");
    }

    if update.position.is_some() || update.playlist.is_some() {
        log::debug!("session_updated: rendering playlist session");
        update_playlist_sessions().await;

        log::debug!("handle_session_update: position or playlist updated");
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(track) = track {
            if let Err(e) = renderer
                .emit_event("play-track".to_string(), Some(track.track_id.to_string()))
                .await
            {
                log::error!("Failed to emit event: {e:?}");
            }
        } else if let Err(e) = renderer.emit_event("unplay-track".to_string(), None).await {
            log::error!("Failed to emit event: {e:?}");
        }
    }
}

async fn update_playlist_sessions() {
    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return;
    };

    let container = moosicbox_app_native_ui::playback_sessions::playback_sessions(
        &connection.api_url,
        &STATE.current_sessions.read().await,
    );
    let view = View::builder().with_fragment(container).build();
    let response = RENDERER.get().unwrap().render(view).await;
    if let Err(e) = response {
        log::error!("Failed to render: {e:?}");
    }
}
