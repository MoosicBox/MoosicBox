use std::{fmt::Display, sync::OnceLock};

use moosicbox_async_service::Arc;
use moosicbox_player::player::Playback;
use moosicbox_session::models::UpdateSession;
use moosicbox_ws::{update_session, WebsocketSender};
use service::Commander as _;
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::RwLock;

use crate::{ws::server::WsServerHandle, DB};

pub static PLAYBACK_EVENT_HANDLE: OnceLock<service::Handle> = OnceLock::new();

pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    let update = update.clone();

    if let Err(err) = PLAYBACK_EVENT_HANDLE
        .get()
        .unwrap()
        .send_command(Command::UpdateSession { update })
    {
        moosicbox_assert::die_or_error!("Failed to broadcast update_session: {err:?}");
    }
}

#[derive(Debug, EnumString, AsRefStr)]
pub enum Command {
    UpdateSession { update: UpdateSession },
}

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

                let db = if let Some(db) = DB.read().unwrap().as_ref() {
                    db.clone()
                } else {
                    log::error!("No DB connection");
                    return Err(
                        std::io::Error::new(std::io::ErrorKind::Other, "No DB connection").into(),
                    );
                };
                if let Err(err) =
                    update_session(&**db, &ctx.read().await.sender, None, &update).await
                {
                    moosicbox_assert::die_or_error!("Failed to update_session: {err:?}");
                }
            }
        }
        Ok(())
    }
}

pub struct Context<Sender: WebsocketSender> {
    sender: Sender,
}

impl<Sender: WebsocketSender> Context<Sender> {
    pub const fn new(sender: Sender) -> Self {
        Self { sender }
    }
}
