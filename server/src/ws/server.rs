//! A multi-room chat server.

use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use kanal::OneshotSender;
use log::{debug, error, info};
use moosicbox_core::app::Db;
use moosicbox_ws::api::{
    WebsocketConnectError, WebsocketContext, WebsocketDisconnectError, WebsocketMessageError,
    WebsocketSendError, WebsocketSender,
};
use rand::{thread_rng, Rng as _};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::ws::{ConnId, Msg, RoomId};

impl WebsocketSender for ChatServer {
    fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<usize>().unwrap();
        debug!("Sending to {id}");
        self.send_message_to(id, data.to_string())?;
        Ok(())
    }

    fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        self.send_system_message("main", 0, data.to_string());
        Ok(())
    }

    fn send_all_except(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        self.send_system_message(
            "main",
            connection_id.parse::<usize>().unwrap(),
            data.to_string(),
        );
        Ok(())
    }
}

/// A command received by the [`ChatServer`].
#[derive(Debug)]
pub enum Command {
    Connect {
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: OneshotSender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
    },

    List {
        res_tx: OneshotSender<Vec<RoomId>>,
    },

    Join {
        conn: ConnId,
        room: RoomId,
        res_tx: OneshotSender<()>,
    },

    Send {
        msg: Msg,
        conn: ConnId,
        res_tx: OneshotSender<()>,
    },

    Broadcast {
        msg: Msg,
        res_tx: OneshotSender<()>,
    },

    BroadcastExcept {
        msg: Msg,
        conn: ConnId,
        res_tx: OneshotSender<()>,
    },

    Message {
        msg: Msg,
        conn: ConnId,
        res_tx: OneshotSender<()>,
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

    db: Arc<Db>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: mpsc::UnboundedReceiver<Command>,
}

impl ChatServer {
    pub fn new(db: Arc<Db>) -> (Self, ChatServerHandle) {
        // create empty server
        let mut rooms = HashMap::with_capacity(4);

        // create default room
        rooms.insert("main".to_owned(), HashSet::new());

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                rooms,
                db,
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
            },
            ChatServerHandle { cmd_tx },
        )
    }

    /// Send message to users in a room.
    ///
    /// `skip` is used to prevent messages triggered by a connection also being received by it.
    fn send_system_message(&self, room: &str, skip: ConnId, msg: impl Into<String>) {
        if let Some(sessions) = self.rooms.get(room) {
            let msg = msg.into();

            for conn_id in sessions {
                if *conn_id != skip {
                    if let Some(tx) = self.sessions.get(conn_id) {
                        // errors if client disconnected abruptly and hasn't been timed-out yet
                        let _ = tx.send(msg.clone());
                    }
                }
            }
        }
    }

    /// Send message directly to the user.
    fn send_message_to(
        &self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketSendError> {
        if let Some(session) = self.sessions.get(&id) {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            let _ = session.send(msg.into());
        }

        Ok(())
    }

    fn on_message(
        &mut self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        let connection_id = id.to_string();
        let context = WebsocketContext { connection_id };
        let payload = msg.into();
        let body = serde_json::from_str::<Value>(&payload)
            .map_err(|e| WebsocketMessageError::InvalidPayload(payload, e.to_string()))?;

        moosicbox_ws::api::process_message(&self.db.clone(), body, context, self)?;

        Ok(())
    }

    /// Register new session and assign unique ID to this session
    fn connect(&mut self, tx: mpsc::UnboundedSender<Msg>) -> Result<ConnId, WebsocketConnectError> {
        info!("Someone joined");

        // register session with random connection ID
        let id = thread_rng().gen::<usize>();
        self.sessions.insert(id, tx.clone());

        // auto join session to main room
        self.rooms.entry("main".to_owned()).or_default().insert(id);

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst);
        debug!("Visitor count: {}", count + 1);

        let connection_id = id.to_string();
        let context = WebsocketContext { connection_id };

        moosicbox_ws::api::connect(&self.db.clone(), self, &context)?;

        // send id back
        Ok(id)
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    fn disconnect(&mut self, conn_id: ConnId) -> Result<(), WebsocketDisconnectError> {
        info!("Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst);
        debug!("Visitor count: {}", count - 1);

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

        let connection_id = conn_id.to_string();
        let context = WebsocketContext { connection_id };

        moosicbox_ws::api::disconnect(&self.db.clone(), self, &context)?;

        Ok(())
    }

    /// Returns list of created room names.
    fn list_rooms(&mut self) -> Vec<String> {
        self.rooms.keys().cloned().collect()
    }

    /// Join room, send disconnect message to old room send join message to new room.
    fn join_room(&mut self, conn_id: ConnId, room: String) {
        let mut rooms = Vec::new();

        // remove session from all rooms
        for (n, sessions) in &mut self.rooms {
            if sessions.remove(&conn_id) {
                rooms.push(n.to_owned());
            }
        }
        // send message to other users
        for room in rooms {
            self.send_system_message(&room, 0, "Someone disconnected");
        }

        self.rooms.entry(room.clone()).or_default().insert(conn_id);

        self.send_system_message(&room, conn_id, "Someone connected");
    }

    pub fn process_command(&mut self, cmd: Command) -> io::Result<()> {
        match cmd {
            Command::Connect { conn_tx, res_tx } => {
                if let Err(error) = self.connect(conn_tx).map(|conn_id| res_tx.send(conn_id)) {
                    error!("Failed to connect: {:?}", error);
                }
            }

            Command::Disconnect { conn } => {
                if let Err(error) = self.disconnect(conn) {
                    error!("Failed to disconnect connection {conn}: {:?}", error);
                }
            }

            Command::List { res_tx } => {
                let _ = res_tx.send(self.list_rooms());
            }

            Command::Join { conn, room, res_tx } => {
                self.join_room(conn, room);
                let _ = res_tx.send(());
            }

            Command::Send { msg, conn, res_tx } => {
                if let Err(error) = self.send(&conn.to_string(), &msg) {
                    error!("Failed to send message to {conn} {msg:?}: {error:?}",);
                }
                let _ = res_tx.send(());
            }

            Command::Broadcast { msg, res_tx } => {
                if let Err(error) = self.send_all(&msg) {
                    error!("Failed to broadcast message {msg:?}: {error:?}",);
                }
                let _ = res_tx.send(());
            }

            Command::BroadcastExcept { msg, conn, res_tx } => {
                if let Err(error) = self.send_all_except(&conn.to_string(), &msg) {
                    error!("Failed to broadcast message {msg:?}: {error:?}",);
                }
                let _ = res_tx.send(());
            }

            Command::Message { conn, msg, res_tx } => {
                if let Err(error) = self.on_message(conn, msg.clone()) {
                    error!(
                        "Failed to process message from {}: {msg:?}: {error:?}",
                        conn
                    );
                }
                let _ = res_tx.send(());
            }
        }

        Ok(())
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            self.process_command(cmd)?;
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

impl WebsocketSender for ChatServerHandle {
    fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<usize>().unwrap();
        self.send(id, data.to_string());
        Ok(())
    }

    fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        self.broadcast(data.to_string());
        Ok(())
    }

    fn send_all_except(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        self.broadcast_except(connection_id.parse::<usize>().unwrap(), data.to_string());
        Ok(())
    }
}

impl ChatServerHandle {
    /// Register client message sender and obtain connection ID.
    pub fn connect(&self, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Connect { conn_tx, res_tx })
            .unwrap();

        // unwrap: chat server does not drop out response channel
        res_rx.recv().unwrap()
    }

    /// List all created rooms.
    pub fn list_rooms(&self) -> Vec<String> {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::List { res_tx }).unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap()
    }

    /// Join `room`, creating it if it does not exist.
    pub fn join_room(&self, conn: ConnId, room: impl Into<String>) {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Join {
                conn,
                room: room.into(),
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap();
    }

    pub fn send(&self, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Send {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap();
    }

    pub fn broadcast(&self, msg: impl Into<String>) {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Broadcast {
                msg: msg.into(),
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap();
    }

    pub fn broadcast_except(&self, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::BroadcastExcept {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap();
    }

    /// Broadcast message to current room.
    pub fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = kanal::oneshot();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.recv().unwrap();
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub fn disconnect(&self, conn: ConnId) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }
}
