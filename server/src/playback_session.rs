use moosicbox_core::sqlite::models::UpdateSession;
use moosicbox_player::player::Playback;
use moosicbox_ws::api::update_session;
use once_cell::sync::Lazy;

use crate::{CHAT_SERVER_HANDLE, DB};

pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    static RT: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap()
    });

    let binding = CHAT_SERVER_HANDLE.read().unwrap_or_else(|e| e.into_inner());
    let sender = binding.as_ref().unwrap();

    RT.block_on(async move {
        if let Err(err) = update_session(DB.get().unwrap(), sender, None, update).await {
            log::error!("Failed to broadcast update_session: {err:?}");
        }
    })
}
