use moosicbox_core::sqlite::models::UpdateSession;
use moosicbox_player::player::PlaybackEvent;
use moosicbox_ws::api::update_session;

use crate::{CHAT_SERVER_HANDLE, DB};

pub fn on_playback_event(event: PlaybackEvent) {
    match event {
        PlaybackEvent::ProgressUpdate(playback, old) => {
            if let Some(session_id) = playback.session_id {
                if playback.progress as usize != old as usize {
                    let binding = CHAT_SERVER_HANDLE.read().unwrap_or_else(|e| e.into_inner());
                    let sender = binding.as_ref().unwrap();

                    if let Err(err) = update_session(
                        DB.get().unwrap(),
                        sender,
                        None,
                        &UpdateSession {
                            session_id: session_id as i32,
                            name: None,
                            active: None,
                            playing: None,
                            position: None,
                            seek: Some(playback.progress as i32),
                            playlist: None,
                        },
                    ) {
                        log::error!("Failed to broadcast update_session: {err:?}");
                    }
                }
            }
        }
        PlaybackEvent::PositionUpdate(_playback, _pos) => {}
    }
}
