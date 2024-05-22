use moosicbox_core::sqlite::models::UpdateSession;
use moosicbox_player::player::Playback;
use moosicbox_ws::update_session;
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

    let update = update.clone();

    RT.spawn(async move {
        let db = if let Some(db) = DB.read().unwrap().as_ref() {
            db.clone()
        } else {
            log::error!("No DB connection");
            return;
        };

        let sender = CHAT_SERVER_HANDLE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .unwrap()
            .clone();

        if let Err(err) = update_session(&**db, &sender, None, &update).await {
            log::error!("Failed to broadcast update_session: {err:?}");
        }
    });
}
