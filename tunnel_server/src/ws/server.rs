//! A multi-room chat server.

use std::{
    collections::{HashMap, HashSet},
    fmt, io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use log::{debug, error, info};
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::{
    mpsc::{self, error::SendError},
    oneshot,
};

use crate::{
    ws::{ConnId, Msg, RoomId},
    CONN_ID,
};

/// A command received by the [`ChatServer`].
#[derive(Debug)]
enum Command {
    Connect {
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: oneshot::Sender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
    },

    Message {
        msg: Msg,
        conn: ConnId,
        res_tx: oneshot::Sender<()>,
    },
}

/// A multi-room chat server.
///
/// Contains the logic of how connections chat with each other plus room management.
///
/// Call and spawn [`run`](Self::run) to start processing commands.
#[derive(Debug)]
pub struct ChatServer {
    /// Map of connection IDs to their message receivers.
    sessions: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,

    /// Map of room name to participant IDs in that room.
    rooms: HashMap<RoomId, HashSet<ConnId>>,
    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: mpsc::UnboundedReceiver<Command>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
    GetConnectionId,
    TunnelRequest,
}

impl fmt::Display for InboundMessageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    #[error("Missing message type")]
    MissingMessageType,
    #[error("Invalid message type")]
    InvalidMessageType,
    #[error("Invalid payload: '{0}' ({1})")]
    InvalidPayload(String, String),
    #[error(transparent)]
    WebsocketSend(#[from] SendError<String>),
}

impl ChatServer {
    pub fn new() -> (Self, ChatServerHandle) {
        // create empty server
        let mut rooms = HashMap::with_capacity(4);

        // create default room
        rooms.insert("main".to_owned(), HashSet::new());

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                rooms,
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
            },
            ChatServerHandle { cmd_tx },
        )
    }

    /// Send message directly to the user.
    async fn send_message_to(
        &self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), SendError<String>> {
        if let Some(session) = self.sessions.get(&id) {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(msg.into())?;
        }

        Ok(())
    }

    async fn on_message(
        &mut self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        let payload = msg.into();
        let body = serde_json::from_str::<Value>(&payload)
            .map_err(|e| WebsocketMessageError::InvalidPayload(payload, e.to_string()))?;
        let message_type = serde_json::from_str::<InboundMessageType>(
            format!(
                "\"{}\"",
                body.get("type")
                    .ok_or(WebsocketMessageError::MissingMessageType)?
                    .as_str()
                    .ok_or(WebsocketMessageError::InvalidMessageType)?
            )
            .as_str(),
        )
        .map_err(|_| WebsocketMessageError::InvalidMessageType)?;

        let payload = body.get("payload");
        debug!("Received Message from {id} [{message_type}] {payload:?}");

        match message_type {
            InboundMessageType::Ping => {}
            InboundMessageType::GetConnectionId => {
                self.send_message_to(
                    id,
                    json!({
                        "type": "CONNECTION_ID",
                        "payload": id
                    })
                    .to_string(),
                )
                .await?
            }
            InboundMessageType::TunnelRequest => {
                self.send_message_to(
                    id,
                    json!({
                        "type": "TUNNEL_REQUEST",
                        "id": body.get("id"),
                        "path": body.get("path"),
                        "payload": payload
                    })
                    .to_string(),
                )
                .await?
            }
        }

        Ok(())
    }

    /// Register new session and assign unique ID to this session
    async fn connect(&mut self, tx: mpsc::UnboundedSender<Msg>) -> ConnId {
        // register session with random connection ID
        let id = thread_rng().gen::<usize>();

        info!("Someone joined {id}");
        CONN_ID.lock().unwrap().replace(id);

        self.sessions.insert(id, tx.clone());

        // auto join session to main room
        self.rooms.entry("main".to_owned()).or_default().insert(id);

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst);
        info!("Visitor count: {}", count + 1);

        // send id back
        id
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    async fn disconnect(&mut self, conn_id: ConnId) {
        info!("Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst);
        info!("Visitor count: {}", count - 1);

        let mut rooms: Vec<String> = Vec::new();

        // remove sender
        if self.sessions.remove(&conn_id).is_some() {
            // remove session from all rooms
            for (name, sessions) in &mut self.rooms {
                if sessions.remove(&conn_id) {
                    rooms.push(name.to_owned());
                }
            }
        }
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                Command::Connect { conn_tx, res_tx } => {
                    if let Err(error) = res_tx.send(self.connect(conn_tx).await) {
                        error!("Failed to connect {error:?}");
                    }
                }

                Command::Disconnect { conn } => self.disconnect(conn).await,

                Command::Message { conn, msg, res_tx } => {
                    if let Err(error) = self.on_message(conn, msg.clone()).await {
                        error!(
                            "Failed to process message from {}: {msg:?}: {error:?}",
                            conn
                        );
                    }
                    let _ = res_tx.send(());
                }
            }
        }

        Ok(())
    }
}

/// Handle and command sender for chat server.
///
/// Reduces boilerplate of setting up response channels in WebSocket handlers.
#[derive(Debug, Clone)]
pub struct ChatServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
}

impl ChatServerHandle {
    /// Register client message sender and obtain connection ID.
    pub async fn connect(&self, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Connect { conn_tx, res_tx })
            .unwrap();

        // unwrap: chat server does not drop out response channel
        res_rx.await.unwrap()
    }

    /// Broadcast message to current room.
    pub async fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = oneshot::channel();

        debug!("Sending message to {conn}");
        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.await.unwrap();
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub fn disconnect(&self, conn: ConnId) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }
}
