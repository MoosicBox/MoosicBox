use moosicbox_core::sqlite::models::UpdateSession;
use moosicbox_player::player::Playback;
use moosicbox_ws::{update_session, WebsocketSender};
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

use crate::{ws::server::ChatServerHandle, CHAT_SERVER_HANDLE, DB};

pub static PLAYBACK_EVENT_HANDLER: Lazy<PlaybackEventHandler<ChatServerHandle>> = Lazy::new(|| {
    PlaybackEventHandler::new(
        CHAT_SERVER_HANDLE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .unwrap()
            .clone(),
    )
});

pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    let update = update.clone();

    if let Err(err) = PLAYBACK_EVENT_HANDLER.send(Command::UpdateSession { update }) {
        log::error!("Failed to broadcast update_session: {err:?}");
    }
}

#[derive(Debug)]
pub enum Command {
    UpdateSession { update: UpdateSession },
}

pub struct PlaybackEventHandler<Sender: WebsocketSender> {
    tx: flume::Sender<Command>,
    rx: flume::Receiver<Command>,
    sender: Sender,
    token: CancellationToken,
}

impl<Sender: WebsocketSender> PlaybackEventHandler<Sender> {
    pub fn new(sender: Sender) -> Self {
        let (tx, rx) = flume::unbounded();

        Self {
            tx,
            rx,
            sender,
            token: CancellationToken::new(),
        }
    }

    pub fn send(&self, cmd: Command) -> Result<(), flume::SendError<Command>> {
        self.tx.send(cmd)
    }

    pub fn shutdown(&self) {
        self.token.cancel();
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        while let Ok(Ok(cmd)) = tokio::select!(
            () = self.token.cancelled() => {
                log::debug!("PlaybackEventHandler was cancelled");
                Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
            }
            cmd = self.rx.recv_async() => {
                Ok(cmd)
            }
        ) {
            log::trace!("Received PlaybackEventHandler command");
            self.process_command(cmd).await?;
        }

        log::debug!("Stopped PlaybackEventHandler");

        Ok(())
    }

    async fn process_command(&self, cmd: Command) -> Result<(), std::io::Error> {
        match cmd {
            Command::UpdateSession { update } => {
                log::debug!("Received UpdateSession command: {update:?}");

                let db = if let Some(db) = DB.read().unwrap().as_ref() {
                    db.clone()
                } else {
                    log::error!("No DB connection");
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "No DB connection",
                    ));
                };
                if let Err(err) = update_session(&**db, &self.sender, None, &update).await {
                    log::error!("Failed to broadcast update_session: {err:?}");
                }
            }
        }

        Ok(())
    }
}
