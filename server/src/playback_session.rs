use moosicbox_core::sqlite::models::UpdateSession;
use moosicbox_player::player::Playback;
use moosicbox_ws::api::update_session;

use crate::{CHAT_SERVER_HANDLE, DB};

pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    let binding = CHAT_SERVER_HANDLE.read().unwrap_or_else(|e| e.into_inner());
    let sender = binding.as_ref().unwrap();

    if let Err(err) = update_session(DB.get().unwrap(), sender, None, update) {
        log::error!("Failed to broadcast update_session: {err:?}");
    }
}
