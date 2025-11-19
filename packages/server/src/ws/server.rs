//! WebSocket server implementation for managing client connections.
//!
//! This module provides a multi-room WebSocket server that manages client connections, message
//! routing, and player action dispatching. It supports multiple profiles and integrates with
//! the `MoosicBox` player system.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use moosicbox_async_service::async_trait;
use moosicbox_ws::{
    PlayerAction, WebsocketContext, WebsocketDisconnectError, WebsocketMessageError,
    WebsocketSendError, WebsocketSender,
};
use serde_json::Value;
use strum_macros::AsRefStr;
use switchy_database::{config::ConfigDatabase, profiles::PROFILES};
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::ws::{ConnId, Msg, RoomId};

#[async_trait]
impl WebsocketSender for WsServer {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<ConnId>().unwrap();
        log::debug!("Sending to {id}");
        self.send_message_to(id, data.to_string());
        for sender in &self.senders {
            sender.send(connection_id, data).await?;
        }
        Ok(())
    }

    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        self.send_system_message("main", 0, data.to_string());
        for sender in &self.senders {
            sender.send_all(data).await?;
        }
        Ok(())
    }

    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        self.send_system_message(
            "main",
            connection_id.parse::<ConnId>().unwrap(),
            data.to_string(),
        );
        for sender in &self.senders {
            sender.send_all_except(connection_id, data).await?;
        }
        Ok(())
    }

    async fn ping(&self) -> Result<(), WebsocketSendError> {
        self.ping_system();
        for sender in &self.senders {
            sender.ping().await?;
        }
        Ok(())
    }
}

/// A command received by the [`WsServer`].
#[derive(Debug, AsRefStr)]
pub enum Command {
    /// Adds a player action to be broadcast to connected clients.
    #[cfg(feature = "player")]
    AddPlayerAction {
        /// Player ID.
        id: u64,
        /// The player action to broadcast.
        action: PlayerAction,
    },

    /// Registers a new WebSocket connection.
    Connect {
        /// Profile name for this connection.
        profile: String,
        /// Channel sender for messages to this connection.
        conn_tx: mpsc::UnboundedSender<Msg>,
        /// Channel to send back the assigned connection ID.
        res_tx: tokio::sync::oneshot::Sender<ConnId>,
    },

    /// Removes a WebSocket connection.
    Disconnect {
        /// Connection ID to disconnect.
        conn: ConnId,
    },

    /// Sends a message to a specific connection.
    Send {
        /// Message to send.
        msg: Msg,
        /// Target connection ID.
        conn: ConnId,
        /// Channel to signal completion.
        res_tx: tokio::sync::oneshot::Sender<()>,
    },

    /// Broadcasts a message to all connections.
    Broadcast {
        /// Message to broadcast.
        msg: Msg,
        /// Channel to signal completion.
        res_tx: tokio::sync::oneshot::Sender<()>,
    },

    /// Broadcasts a message to all connections except one.
    BroadcastExcept {
        /// Message to broadcast.
        msg: Msg,
        /// Connection ID to exclude from the broadcast.
        conn: ConnId,
        /// Channel to signal completion.
        res_tx: tokio::sync::oneshot::Sender<()>,
    },

    /// Processes an incoming message from a connection.
    Message {
        /// The received message.
        msg: Msg,
        /// Connection ID that sent the message.
        conn: ConnId,
        /// Channel to signal completion.
        res_tx: tokio::sync::oneshot::Sender<()>,
    },
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Represents an active WebSocket connection.
///
/// Contains the profile name and message sender channel for a connected client.
#[derive(Debug, Clone)]
struct Connection {
    /// The profile name this connection is using.
    profile: String,
    /// Channel for sending messages to this connection.
    sender: mpsc::UnboundedSender<Msg>,
}

/// A multi-room ws server.
///
/// Contains the logic of how connections ws with each other plus room management.
///
/// Call and spawn [`run`](Self::run) to start processing commands.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct WsServer {
    /// Map of connection IDs to their message receivers.
    connections: BTreeMap<ConnId, Connection>,

    config_db: ConfigDatabase,

    /// Map of room name to participant IDs in that room.
    rooms: BTreeMap<RoomId, BTreeSet<ConnId>>,

    /// Map of profiles to participant IDs using that profile.
    #[allow(unused)]
    profiles: BTreeMap<String, BTreeSet<ConnId>>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: flume::Receiver<Command>,

    senders: Vec<Box<dyn WebsocketSender>>,

    player_actions: Vec<(u64, PlayerAction)>,

    token: CancellationToken,
}

impl WsServer {
    pub fn new(config_db: ConfigDatabase) -> (Self, WsServerHandle) {
        // create empty server
        let mut rooms = BTreeMap::new();

        // create default room
        rooms.insert("main".to_owned(), BTreeSet::new());

        let mut profiles = BTreeMap::new();

        for profile in PROFILES.names() {
            profiles.insert(profile, BTreeSet::new());
        }

        let (cmd_tx, cmd_rx) = flume::unbounded();
        let token = CancellationToken::new();
        let handle = WsServerHandle {
            cmd_tx,
            token: token.clone(),
        };

        (
            Self {
                connections: BTreeMap::new(),
                config_db,
                rooms,
                profiles,
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
                senders: vec![],
                player_actions: vec![],
                token,
            },
            handle,
        )
    }

    #[cfg(feature = "player")]
    pub fn add_player_action(&mut self, id: u64, action: PlayerAction) {
        self.player_actions.push((id, action));
    }

    #[cfg(feature = "tunnel")]
    pub fn add_sender(&mut self, sender: Box<dyn WebsocketSender>) {
        self.senders.push(sender);
    }

    #[allow(clippy::unused_self)]
    fn ping_system(&self) {
        log::trace!("ping_system: pong");
    }

    /// Send message to users in a room.
    ///
    /// `skip` is used to prevent messages triggered by a connection also being received by it.
    fn send_system_message(&self, room: &str, skip: ConnId, msg: impl Into<String>) {
        if let Some(sessions) = self.rooms.get(room) {
            let msg = msg.into();

            for conn_id in sessions {
                if *conn_id != skip
                    && let Some(Connection { sender, .. }) = self.connections.get(conn_id)
                {
                    // errors if client disconnected abruptly and hasn't been timed-out yet
                    let _ = sender.send(msg.clone());
                }
            }
        }
    }

    /// Send message directly to the user.
    fn send_message_to(&self, id: ConnId, msg: impl Into<String>) {
        if let Some(Connection { sender, .. }) = self.connections.get(&id) {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            let _ = sender.send(msg.into());
        }
    }

    async fn on_message(
        &self,
        id: ConnId,
        msg: impl Into<String> + Send,
    ) -> Result<(), WebsocketMessageError> {
        let connection_id = id.to_string();
        let profile = self.connections.get(&id).unwrap().profile.clone();
        log::trace!(
            "on_message connection_id={connection_id} player_actions.len={}",
            self.player_actions.len()
        );
        let context = WebsocketContext {
            connection_id,
            profile: Some(profile),
            player_actions: self.player_actions.clone(),
        };
        let payload = msg.into();
        let body = serde_json::from_str::<Value>(&payload)
            .map_err(|e| WebsocketMessageError::InvalidPayload(payload, e.to_string()))?;

        moosicbox_ws::process_message(&self.config_db, body, context, self).await?;

        Ok(())
    }

    /// Register new session and assign unique ID to this session
    fn connect(&mut self, profile: String, tx: mpsc::UnboundedSender<Msg>) -> ConnId {
        log::debug!("Someone joined");

        // register session with random connection ID
        let id = switchy_random::rng().next_u64();
        self.connections.insert(
            id,
            Connection {
                profile: profile.clone(),
                sender: tx,
            },
        );

        // auto join session to main room
        self.rooms.entry("main".to_owned()).or_default().insert(id);

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst);
        log::debug!("Visitor count: {}", count + 1);

        let connection_id = id.to_string();
        let context = WebsocketContext {
            connection_id,
            profile: Some(profile),
            player_actions: self.player_actions.clone(),
        };

        let _ = moosicbox_ws::connect(self, &context);

        // send id back
        id
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    async fn disconnect(&mut self, conn_id: ConnId) -> Result<(), WebsocketDisconnectError> {
        log::debug!("Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst);
        log::debug!("Visitor count: {}", count - 1);

        // remove sender
        if self.connections.remove(&conn_id).is_some() {
            // remove session from all rooms
            for sessions in self.rooms.values_mut() {
                sessions.remove(&conn_id);
            }
        }

        let connection_id = conn_id.to_string();
        let context = WebsocketContext {
            connection_id,
            profile: None,
            player_actions: self.player_actions.clone(),
        };

        moosicbox_ws::disconnect(&self.config_db, self, &context).await?;

        Ok(())
    }

    #[allow(clippy::cognitive_complexity)]
    async fn process_command(ctx: Arc<RwLock<Self>>, cmd: Command) -> io::Result<()> {
        let cmd_str = cmd.to_string();

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("process_command: cmd={cmd:?}");
        } else {
            log::debug!("process_command: cmd={cmd_str}");
        }

        match cmd {
            #[cfg(feature = "player")]
            Command::AddPlayerAction { id, action } => {
                ctx.write().await.add_player_action(id, action);
                log::debug!("Added a player action with id={id}");
            }

            Command::Connect {
                profile,
                conn_tx,
                res_tx,
            } => {
                let conn_id = ctx.write().await.connect(profile, conn_tx);
                res_tx
                    .send(conn_id)
                    .map_err(|e| std::io::Error::other(format!("Failed to send: {e:?}")))?;
            }

            Command::Disconnect { conn } => {
                let response = ctx.write().await.disconnect(conn).await;
                if let Err(error) = response {
                    moosicbox_assert::die_or_error!(
                        "Failed to disconnect connection {conn}: {:?}",
                        error
                    );
                }
            }

            Command::Send { msg, conn, res_tx } => {
                let response = ctx.read().await.send(&conn.to_string(), &msg).await;
                if let Err(error) = response {
                    moosicbox_assert::die_or_error!(
                        "Failed to send message to {conn} {msg:?}: {error:?}",
                    );
                }
                let _ = res_tx.send(());
            }

            Command::Broadcast { msg, res_tx } => {
                let response = ctx.read().await.send_all(&msg).await;
                if let Err(error) = response {
                    moosicbox_assert::die_or_error!(
                        "Failed to broadcast message {msg:?}: {error:?}",
                    );
                }
                let _ = res_tx.send(());
            }

            Command::BroadcastExcept { msg, conn, res_tx } => {
                let response = ctx
                    .read()
                    .await
                    .send_all_except(&conn.to_string(), &msg)
                    .await;
                if let Err(error) = response {
                    moosicbox_assert::die_or_error!(
                        "Failed to broadcast message {msg:?}: {error:?}",
                    );
                }
                let _ = res_tx.send(());
            }

            Command::Message { conn, msg, res_tx } => {
                let response = ctx.read().await.on_message(conn, msg.clone()).await;
                if let Err(error) = response {
                    if log::log_enabled!(log::Level::Debug) {
                        moosicbox_assert::die_or_error!(
                            "Failed to process message from {}: {msg:?}: {error:?}",
                            conn
                        );
                    } else {
                        moosicbox_assert::die_or_error!(
                            "Failed to process message from {}: {msg:?}: {error:?} ({:?})",
                            conn,
                            msg
                        );
                    }
                }
                let _ = res_tx.send(());
            }
        }

        log::debug!("process_command: Finished processing cmd {cmd_str}");

        Ok(())
    }

    pub async fn run(self) -> io::Result<()> {
        let token = self.token.clone();
        let cmd_rx = self.cmd_rx.clone();
        let ctx = Arc::new(RwLock::new(self));
        while let Ok(Ok(cmd)) = tokio::select!(
            () = token.cancelled() => {
                log::debug!("WsServer was cancelled");
                Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
            }
            cmd = cmd_rx.recv_async() => { Ok(cmd) }
        ) {
            log::trace!("Received WsServer command {cmd}");
            switchy_async::runtime::Handle::current().spawn_with_name(
                "server: WsServer process_command",
                Self::process_command(ctx.clone(), cmd),
            );
        }

        log::debug!("Stopped WsServer");

        Ok(())
    }
}

/// Handle and command sender for ws server.
///
/// Reduces boilerplate of setting up response channels in `WebSocket` handlers.
#[derive(Debug, Clone)]
pub struct WsServerHandle {
    cmd_tx: flume::Sender<Command>,
    token: CancellationToken,
}

#[async_trait]
impl WebsocketSender for WsServerHandle {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<ConnId>().unwrap();
        self.send(id, data.to_string()).await;
        Ok(())
    }

    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("Broadcasting message to all: {data}");
        } else {
            log::debug!("Broadcasting message to all");
        }
        self.broadcast(data.to_string()).await;
        Ok(())
    }

    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("Broadcasting message to all except {connection_id}: {data}");
        } else {
            log::debug!("Broadcasting message to all except {connection_id}");
        }
        self.broadcast_except(connection_id.parse::<ConnId>().unwrap(), data.to_string())
            .await;
        Ok(())
    }

    async fn ping(&self) -> Result<(), WebsocketSendError> {
        self.ping()
            .await
            .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        Ok(())
    }
}

impl WsServerHandle {
    #[cfg(feature = "player")]
    pub async fn add_player_action(&self, player_id: u64, action: PlayerAction) {
        log::trace!("Sending AddPlayerAction command id={player_id}");

        if let Err(e) = self
            .cmd_tx
            .send_async(Command::AddPlayerAction {
                id: player_id,
                action,
            })
            .await
        {
            moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
        }
    }

    /// Register client message sender and obtain connection ID.
    pub async fn connect(&self, profile: String, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
        log::trace!("Sending Connect command");

        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        switchy_async::runtime::Handle::current().spawn_with_name("ws server connect", {
            let cmd_tx = self.cmd_tx.clone();
            async move {
                if let Err(e) = cmd_tx
                    .send_async(Command::Connect {
                        profile,
                        conn_tx,
                        res_tx,
                    })
                    .await
                {
                    moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
                }
            }
        });

        res_rx.await.unwrap_or_else(|e| {
            moosicbox_assert::die_or_panic!("Failed to recv response from ws server: {e:?}")
        })
    }

    pub async fn send(&self, conn: ConnId, msg: impl Into<String> + Send) {
        log::trace!("Sending Send command");
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        switchy_async::runtime::Handle::current().spawn_with_name("ws server send", {
            let cmd_tx = self.cmd_tx.clone();
            let msg = msg.into();
            async move {
                if let Err(e) = cmd_tx.send_async(Command::Send { msg, conn, res_tx }).await {
                    moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
                }
            }
        });

        res_rx.await.unwrap_or_else(|e| {
            moosicbox_assert::die_or_error!("Failed to recv response from ws server: {e:?}");
        });
    }

    pub async fn broadcast(&self, msg: impl Into<String> + Send) {
        log::trace!("Sending Broadcast command");
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        switchy_async::runtime::Handle::current().spawn_with_name("ws server broadcast", {
            let cmd_tx = self.cmd_tx.clone();
            let msg = msg.into();
            async move {
                if let Err(e) = cmd_tx.send_async(Command::Broadcast { msg, res_tx }).await {
                    moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
                }
            }
        });

        res_rx.await.unwrap_or_else(|e| {
            moosicbox_assert::die_or_error!("Failed to recv response from ws server: {e:?}");
        });
    }

    pub async fn broadcast_except(&self, conn: ConnId, msg: impl Into<String> + Send) {
        log::trace!("Sending BroadcastExcept command");
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        switchy_async::runtime::Handle::current().spawn_with_name("ws server broadcast_except", {
            let cmd_tx = self.cmd_tx.clone();
            let msg = msg.into();
            async move {
                if let Err(e) = cmd_tx
                    .send_async(Command::BroadcastExcept { msg, conn, res_tx })
                    .await
                {
                    moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
                }
            }
        });

        res_rx.await.unwrap_or_else(|e| {
            moosicbox_assert::die_or_error!("Failed to recv response from ws server: {e:?}");
        });
    }

    /// Broadcast message to current room.
    pub async fn send_message(&self, conn: ConnId, msg: impl Into<String> + Send) {
        log::trace!("Sending Message command");
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        switchy_async::runtime::Handle::current().spawn_with_name("ws server send_message", {
            let cmd_tx = self.cmd_tx.clone();
            let msg = msg.into();
            async move {
                if let Err(e) = cmd_tx
                    .send_async(Command::Message { msg, conn, res_tx })
                    .await
                {
                    moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
                }
            }
        });

        res_rx.await.unwrap_or_else(|e| {
            moosicbox_assert::die_or_error!("Failed to recv response from ws server: {e:?}");
        });
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub async fn disconnect(&self, conn: ConnId) {
        log::trace!("Sending Disconnect command");

        if let Err(e) = self.cmd_tx.send_async(Command::Disconnect { conn }).await {
            moosicbox_assert::die_or_error!("Failed to send command: {e:?}");
        }
    }

    pub fn shutdown(&self) {
        self.token.cancel();
    }
}
