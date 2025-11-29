use std::{fmt::Display, sync::LazyLock};

use moosicbox_async_service::Arc;
use moosicbox_player::Playback;
use moosicbox_session::models::UpdateSession;
use moosicbox_ws::{WebsocketSender, update_session};
use service::Commander as _;
use strum_macros::{AsRefStr, EnumString};
use switchy_async::sync::RwLock;
use switchy_database::profiles::PROFILES;

use crate::{CONFIG_DB, ws::server::WsServerHandle};

pub static PLAYBACK_EVENT_HANDLE: LazyLock<Arc<std::sync::RwLock<Option<service::Handle>>>> =
    LazyLock::new(|| Arc::new(std::sync::RwLock::new(None)));

/// Event handler for playback state changes.
///
/// This function is called when playback state changes occur (play, pause, seek, track change,
/// etc.) and dispatches the update to connected WebSocket clients via the playback event service.
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn on_event(update: &UpdateSession, _current: &Playback) {
    let update = update.clone();

    let Some(handle) = PLAYBACK_EVENT_HANDLE.read().unwrap().clone() else {
        return;
    };

    if let Err(err) = handle.send_command(Command::UpdateSession { update }) {
        moosicbox_assert::die_or_error!("Failed to broadcast update_session: {err:?}");
    }
}

/// Commands processed by the playback event service.
#[derive(Debug, EnumString, AsRefStr)]
pub enum Command {
    /// Updates the playback session and broadcasts changes to clients.
    UpdateSession {
        /// The session update to process and broadcast.
        update: UpdateSession,
    },
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub mod service {
    use crate::ws::server::WsServerHandle;

    moosicbox_async_service::async_service!(super::Command, super::Context<WsServerHandle>);
}

#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = service::Error;

    async fn process_command(
        ctx: Arc<RwLock<Context<WsServerHandle>>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command command={command}");
        match command {
            Command::UpdateSession { update } => {
                log::debug!("Received UpdateSession command: {update:?}");

                let config_db = if let Some(config_db) = CONFIG_DB.read().unwrap().as_ref() {
                    config_db.clone()
                } else {
                    log::error!("No DB connection");
                    return Err(std::io::Error::other("No CONFIG_DB connection").into());
                };

                let Some(db) = PROFILES.get(&update.profile) else {
                    log::error!("No DB connection");
                    return Err(std::io::Error::other("No DB connection").into());
                };

                let handle = { ctx.read().await.sender.clone() };

                if let Err(err) = update_session(&config_db, &db, &handle, None, &update).await {
                    moosicbox_assert::die_or_error!("Failed to update_session: {err:?}");
                }
            }
        }
        Ok(())
    }
}

/// Context for the playback event service.
///
/// Contains the WebSocket sender for broadcasting playback updates to clients.
pub struct Context<Sender: WebsocketSender> {
    sender: Sender,
}

impl<Sender: WebsocketSender> Context<Sender> {
    /// Creates a new playback event context with the given WebSocket sender.
    #[must_use]
    pub const fn new(sender: Sender) -> Self {
        Self { sender }
    }
}
