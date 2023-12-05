//! A multi-room chat server.

use std::{
    collections::HashMap,
    fmt, io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use log::{debug, error, info, warn};
use moosicbox_tunnel::tunnel::{TunnelRequest, TunnelResponse, TunnelWsRequest, TunnelWsResponse};
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;
use tokio::sync::{
    mpsc::{self, error::SendError, Sender},
    oneshot,
};

use crate::ws::{
    db::{delete_connection, upsert_connection},
    ConnId, Msg,
};

use super::db::select_connection;

/// A command received by the [`ChatServer`].
#[derive(Debug)]
enum Command {
    Connect {
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: oneshot::Sender<ConnId>,
        client_id: String,
        sender: bool,
    },

    Disconnect {
        conn: ConnId,
    },

    RequestStart {
        request_id: usize,
        sender: Sender<TunnelResponse>,
        headers_sender: Sender<HashMap<String, String>>,
    },

    RequestEnd {
        request_id: usize,
    },

    Response {
        response: TunnelResponse,
    },

    WsRequest {
        request_id: usize,
        conn_id: ConnId,
        client_id: String,
        body: String,
    },

    WsResponse {
        response: TunnelWsResponse,
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
    senders: HashMap<usize, Sender<TunnelResponse>>,
    headers_senders: HashMap<usize, Sender<HashMap<String, String>>>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: mpsc::UnboundedReceiver<Command>,

    ws_requests: HashMap<usize, ConnId>,
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
}

impl fmt::Display for InboundMessageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    #[error("Session {0} not connected")]
    NoSession(ConnId),
    #[error(transparent)]
    WebsocketSend(#[from] SendError<String>),
}

impl ChatServer {
    pub fn new() -> (Self, ChatServerHandle) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                senders: HashMap::new(),
                headers_senders: HashMap::new(),
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
                ws_requests: HashMap::new(),
            },
            ChatServerHandle { cmd_tx },
        )
    }

    /// Send message directly to the user.
    async fn send_message_to(
        &self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        debug!("Sending message to {id}");

        if let Some(session) = self.sessions.get(&id) {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            Ok(session.send(msg.into())?)
        } else {
            Err(WebsocketMessageError::NoSession(id))
        }
    }

    /// Register new session and assign unique ID to this session
    async fn connect(
        &mut self,
        client_id: String,
        sender: bool,
        tx: mpsc::UnboundedSender<Msg>,
    ) -> ConnId {
        // register session with random connection ID
        let id = thread_rng().gen::<usize>();

        info!("Someone joined {id} sender={sender}");

        self.sessions.insert(id, tx.clone());

        if sender {
            upsert_connection(&client_id, &id.to_string());
        }

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst) + 1;
        info!("Visitor count: {count}");

        // send id back
        id
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    async fn disconnect(&mut self, conn_id: ConnId) {
        info!("Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst) - 1;
        info!("Visitor count: {count}");

        delete_connection(&conn_id.to_string());

        // remove sender
        self.sessions.remove(&conn_id);
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                Command::Connect {
                    client_id,
                    conn_tx,
                    res_tx,
                    sender,
                } => {
                    if let Err(error) = res_tx.send(self.connect(client_id, sender, conn_tx).await)
                    {
                        error!("Failed to connect {error:?}");
                    }
                }

                Command::Disconnect { conn } => self.disconnect(conn).await,

                Command::RequestStart {
                    request_id,
                    sender,
                    headers_sender,
                } => {
                    self.senders.insert(request_id, sender);
                    self.headers_senders.insert(request_id, headers_sender);
                }

                Command::RequestEnd { request_id } => {
                    self.senders.remove(&request_id);
                    self.headers_senders.remove(&request_id);
                }

                Command::Response { response } => {
                    let request_id = response.request_id;

                    if let Some(headers) = &response.headers {
                        if let Some(sender) = self.headers_senders.get(&request_id) {
                            if sender.send(headers.clone()).await.is_err() {
                                warn!("Header sender dropped for request {}", request_id);
                                self.headers_senders.remove(&request_id);
                            }
                        } else {
                            error!(
                                "unexpected binary message {} (size {})",
                                request_id,
                                response.bytes.len()
                            );
                        }
                    }

                    if let Some(sender) = self.senders.get(&request_id) {
                        if sender.send(response).await.is_err() {
                            warn!("Sender dropped for request {}", request_id);
                            self.senders.remove(&request_id);
                        }
                    } else {
                        error!(
                            "unexpected binary message {} (size {})",
                            request_id,
                            response.bytes.len()
                        );
                    }
                }

                Command::WsRequest {
                    conn_id,
                    client_id,
                    request_id,
                    body,
                } => {
                    if let Some(client_conn_id) = match select_connection(&client_id)
                        .ok_or_else(|| WsRequestError::MissingClientId(client_id.clone()))
                        .map(|conn| {
                            conn.tunnel_ws_id
                                .parse::<usize>()
                                .map_err(|_| WsRequestError::InvalidClientId(client_id))
                        }) {
                        Ok(Ok(id)) => Some(id),
                        Ok(Err(err)) | Err(err) => {
                            error!("Failed to get connection id from client id: {err:?}");
                            None
                        }
                    } {
                        let value: Value = serde_json::from_str(&body).unwrap();
                        let body = TunnelRequest::WsRequest(TunnelWsRequest {
                            request_id,
                            body: value,
                        });

                        if let Err(error) = self
                            .send_message_to(client_conn_id, serde_json::to_string(&body).unwrap())
                            .await
                        {
                            error!("Failed to send WsRequest to {client_conn_id}: {error:?}");
                        }
                        self.ws_requests.insert(request_id, conn_id);
                    }
                }

                Command::WsResponse { response } => {
                    if let Some(ws_id) = self.ws_requests.get(&response.request_id) {
                        if let Err(error) = self
                            .send_message_to(*ws_id, response.body.to_string())
                            .await
                        {
                            error!("Failed to send WsResponse to {ws_id}: {error:?}");
                        }
                    } else {
                        error!("unexpected ws response {}", response.request_id,);
                    }
                }

                Command::Message { conn, msg, res_tx } => {
                    if let Err(error) = self.send_message_to(conn, &msg).await {
                        error!("Failed to send message to {conn}: {msg:?}: {error:?}");
                    }
                    let _ = res_tx.send(());
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WsRequestError {
    #[error("Missing client ID: {0}")]
    MissingClientId(String),
    #[error("Invalid client ID: {0}")]
    InvalidClientId(String),
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
    pub async fn connect(
        &self,
        client_id: &str,
        sender: bool,
        conn_tx: mpsc::UnboundedSender<String>,
    ) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Connect {
                conn_tx,
                res_tx,
                client_id: client_id.to_string(),
                sender,
            })
            .unwrap();

        // unwrap: chat server does not drop out response channel
        res_rx.await.unwrap()
    }

    /// Broadcast message to current room.
    pub async fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = oneshot::channel();

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

    pub async fn ws_request(
        &self,
        conn_id: usize,
        client_id: &str,
        msg: impl Into<String>,
    ) -> Result<(), WsRequestError> {
        let request_id = thread_rng().gen::<usize>();

        self.cmd_tx
            .send(Command::WsRequest {
                request_id,
                conn_id,
                client_id: client_id.to_string(),
                body: msg.into(),
            })
            .unwrap();
        Ok(())
    }

    pub async fn ws_response(&self, response: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.cmd_tx.send(Command::WsResponse { response }).unwrap();

        Ok(())
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub fn disconnect(&self, conn: ConnId) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }

    pub fn request_start(
        &self,
        request_id: usize,
        sender: Sender<TunnelResponse>,
        headers_sender: Sender<HashMap<String, String>>,
    ) {
        self.cmd_tx
            .send(Command::RequestStart {
                request_id,
                sender,
                headers_sender,
            })
            .unwrap();
    }

    pub fn request_end(&self, request_id: usize) {
        self.cmd_tx
            .send(Command::RequestEnd { request_id })
            .unwrap();
    }

    pub fn response(&self, response: TunnelResponse) {
        self.cmd_tx.send(Command::Response { response }).unwrap();
    }
}
