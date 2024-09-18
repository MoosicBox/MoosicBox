use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, LazyLock,
    },
};

use moosicbox_tunnel::{
    TunnelAbortRequest, TunnelRequest, TunnelResponse, TunnelWsRequest, TunnelWsResponse,
};
use moosicbox_tunnel_server::CANCELLATION_TOKEN;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::sync::{
    mpsc::{self, error::SendError, UnboundedSender},
    oneshot, RwLock,
};
use tokio_util::sync::CancellationToken;

use crate::db::{delete_connection, select_connection, upsert_connection, DatabaseError};
use crate::ws::{ConnId, Msg};

use self::service::{Commander, CommanderError};

/// A command received by the [`WsServer`].
#[derive(Debug, AsRefStr)]
pub enum Command {
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
        sender: UnboundedSender<TunnelResponse>,
        headers_sender: oneshot::Sender<RequestHeaders>,
        abort_request_token: CancellationToken,
    },

    RequestEnd {
        request_id: usize,
    },

    Response {
        response: TunnelResponse,
        conn_id: ConnId,
    },

    WsRequest {
        request_id: usize,
        conn_id: ConnId,
        client_id: String,
        body: String,
        profile: Option<String>,
    },

    WsMessage {
        message: TunnelWsResponse,
    },

    WsResponse {
        response: TunnelWsResponse,
    },

    Message {
        msg: Msg,
        conn: ConnId,
    },
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub mod service {
    moosicbox_async_service::async_service!(super::Command, super::WsServer);
}

#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = service::Error;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        self.token = CANCELLATION_TOKEN.clone();
        Ok(())
    }

    async fn process_command(
        ctx: Arc<RwLock<WsServer>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command command={command}");
        match command {
            Command::Connect {
                client_id,
                conn_tx,
                res_tx,
                sender,
            } => {
                let mut binding = ctx.write().await;
                let response = binding.connect(client_id, sender, conn_tx).await;
                drop(binding);
                match response {
                    Ok(id) => {
                        log::info!("Successfully connected id={id}");
                        if let Err(error) = res_tx.send(id) {
                            log::error!("Failed to connect {error:?}");
                        }
                    }
                    Err(err) => log::error!("Failed to connect {err:?}"),
                }
            }

            Command::Disconnect { conn } => {
                let mut binding = ctx.write().await;
                let response = binding.disconnect(conn).await;
                drop(binding);
                if let Err(err) = response {
                    log::error!("Failed to disconnect {err:?}");
                }
            }

            Command::RequestStart {
                request_id,
                sender,
                headers_sender,
                abort_request_token,
            } => {
                let mut ctx = ctx.write().await;
                ctx.senders.insert(request_id, sender);
                ctx.headers_senders.insert(request_id, headers_sender);
                ctx.abort_request_tokens
                    .insert(request_id, abort_request_token);
                drop(ctx);
            }

            Command::RequestEnd { request_id } => {
                let mut ctx = ctx.write().await;
                ctx.senders.remove(&request_id);
                ctx.headers_senders.remove(&request_id);
                ctx.abort_request_tokens.remove(&request_id);
                drop(ctx);
            }

            Command::Response { response, conn_id } => {
                let request_id = response.request_id;
                log::debug!("process_command: Handling response for request_id={request_id}");

                if let (Some(status), Some(headers)) = (response.status, &response.headers) {
                    log::debug!("process_command: Response request_id={request_id} status={status} headers={headers:?}");
                    let headers_senders = {
                        let mut ctx = ctx.write().await;
                        let headers_senders = ctx.headers_senders.remove(&request_id);
                        drop(ctx);
                        headers_senders
                    };
                    if let Some(sender) = headers_senders {
                        if sender
                            .send(RequestHeaders {
                                status,
                                headers: headers.clone(),
                            })
                            .is_err()
                        {
                            log::warn!(
                                "process_command: Header sender dropped for request_id={request_id}"
                            );
                            {
                                let mut ctx = ctx.write().await;
                                ctx.headers_senders.remove(&request_id);
                                drop(ctx);
                            }
                            if let Err(err) =
                                ctx.read().await.abort_request(conn_id, request_id).await
                            {
                                log::error!("process_command: Failed to abort request_id={request_id}: {err:?}");
                            }
                        }
                    } else {
                        log::error!(
                            "process_command: unexpected binary message request_id={request_id} (size {})",
                            response.bytes.len()
                        );
                    }
                }

                let sender = ctx.read().await.senders.get(&request_id).cloned();

                if let Some(sender) = sender {
                    let packet_id = response.packet_id;
                    let last = response.last;
                    let status = response.status;
                    log::trace!("process_command: Sending response for request_id={request_id} packet_id={packet_id} last={last} status={status:?}");
                    if sender.send(response).is_err() {
                        log::debug!("process_command: Sender dropped for request_id={request_id}");
                        let mut binding = ctx.write().await;
                        binding.senders.remove(&request_id);
                        drop(binding);
                        if let Err(err) = ctx.read().await.abort_request(conn_id, request_id).await
                        {
                            log::error!(
                                "process_command: Failed to abort request_id={request_id} {err:?}"
                            );
                        }
                    } else {
                        log::trace!("process_command: Sent response for request_id={request_id} packet_id={packet_id} last={last} status={status:?}");
                    }
                } else {
                    log::error!(
                        "process_command: unexpected binary message request_id={request_id} (size {})",
                        response.bytes.len()
                    );
                }
            }

            Command::WsRequest {
                conn_id,
                client_id,
                request_id,
                body,
                profile,
            } => match get_connection_id(&client_id).await {
                Ok(client_conn_id) => {
                    let value: Value = serde_json::from_str(&body).unwrap();
                    let body = TunnelRequest::Ws(TunnelWsRequest {
                        conn_id,
                        request_id,
                        body: value,
                        connection_id: None,
                        profile,
                    });
                    let binding = ctx.read().await;
                    let response = binding
                        .send_message_to(client_conn_id, serde_json::to_string(&body).unwrap())
                        .await;
                    drop(binding);

                    if let Err(error) = response {
                        log::error!("Failed to send WsRequest to {client_conn_id}: {error:?}");
                    }
                    let mut binding = ctx.write().await;
                    binding.ws_requests.insert(request_id, conn_id);
                    drop(binding);
                }
                Err(err) => {
                    log::error!("Failed to get connection id: {err:?}");
                }
            },

            Command::WsMessage { message } => {
                if let Some(to_connection_ids) = message.to_connection_ids {
                    for conn_id in to_connection_ids {
                        let binding = ctx.read().await;
                        let response = binding
                            .send_message_to(conn_id, message.body.to_string())
                            .await;
                        drop(binding);
                        if let Err(error) = response {
                            log::error!("Failed to send WsResponse to {conn_id}: {error:?}");
                        }
                    }
                } else if let Some(exclude_connection_ids) = message.exclude_connection_ids {
                    let binding = ctx.read().await;
                    let response = binding
                        .broadcast_except(&exclude_connection_ids, message.body.to_string())
                        .await;
                    drop(binding);
                    if let Err(error) = response {
                        log::error!("Failed to broadcast_except WsMessage: {error:?}");
                    }
                } else {
                    let binding = ctx.read().await;
                    let response = binding.broadcast(message.body.to_string()).await;
                    drop(binding);
                    if let Err(error) = response {
                        log::error!("Failed to broadcast WsMessage: {error:?}");
                    }
                }
            }

            Command::WsResponse { response } => {
                let binding = ctx.read().await;
                let ws_id = binding.ws_requests.get(&response.request_id).cloned();
                drop(binding);
                if let Some(ws_id) = ws_id {
                    let binding = ctx.read().await;
                    let response = binding
                        .send_message_to(ws_id, response.body.to_string())
                        .await;
                    drop(binding);
                    if let Err(error) = response {
                        log::error!("Failed to send WsResponse to {ws_id}: {error:?}");
                    }
                } else {
                    log::error!("unexpected ws response {}", response.request_id,);
                }
            }

            Command::Message { conn, msg } => {
                let binding = ctx.read().await;
                let response = binding.send_message_to(conn, &msg).await;
                drop(binding);
                if let Err(error) = response {
                    log::error!("Failed to send message to {conn}: {msg:?}: {error:?}");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RequestHeaders {
    pub status: u16,
    pub headers: HashMap<String, String>,
}

/// A multi-room ws server.
///
/// Contains the logic of how connections ws with each other plus room management.
///
/// Call and spawn [`run`](Self::run) to start processing commands.
#[derive(Debug)]
pub struct WsServer {
    /// Map of connection IDs to their message receivers.
    sessions: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,
    clients: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,
    senders: HashMap<usize, UnboundedSender<TunnelResponse>>,
    headers_senders: HashMap<usize, oneshot::Sender<RequestHeaders>>,
    abort_request_tokens: HashMap<usize, CancellationToken>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

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

impl WsServer {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            clients: HashMap::new(),
            senders: HashMap::new(),
            headers_senders: HashMap::new(),
            abort_request_tokens: HashMap::new(),
            visitor_count: Arc::new(AtomicUsize::new(0)),
            ws_requests: HashMap::new(),
        }
    }

    async fn abort_request(
        &self,
        id: ConnId,
        request_id: usize,
    ) -> Result<(), WebsocketMessageError> {
        log::debug!("Aborting request {request_id} (conn_id={id})");
        if let Some(abort_token) = self.abort_request_tokens.get(&request_id) {
            abort_token.cancel();
        } else {
            log::debug!("No abort token for request {request_id}");
        }
        let body = TunnelRequest::Abort(TunnelAbortRequest { request_id });
        self.send_message_to(id, serde_json::to_string(&body).unwrap())
            .await
    }

    /// Send message directly to the user.
    async fn send_message_to(
        &self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        if let Some(session) = self.sessions.get(&id) {
            let message = msg.into();
            log::debug!("Sending message to {id} size={}", message.len());
            // errors if client disconnected abruptly and hasn't been timed-out yet
            Ok(session.send(message)?)
        } else {
            Err(WebsocketMessageError::NoSession(id))
        }
    }

    /// Send message directly to the user.
    async fn broadcast(&self, msg: impl Into<String>) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message");
        let message = msg.into();

        for session in self.clients.values() {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?
        }
        Ok(())
    }

    /// Send message directly to the user.
    async fn broadcast_except(
        &self,
        ids: &[ConnId],
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message except {ids:?}");
        let message = msg.into();

        for (id, session) in &self.clients {
            if ids.iter().any(|exclude| *exclude == *id) {
                continue;
            }
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?
        }
        Ok(())
    }

    /// Register new session and assign unique ID to this session
    async fn connect(
        &mut self,
        client_id: String,
        sender: bool,
        tx: mpsc::UnboundedSender<Msg>,
    ) -> Result<ConnId, DatabaseError> {
        // register session with random connection ID
        let id = thread_rng().gen::<usize>();

        log::debug!("connect: Someone joined {id} sender={sender}");

        self.sessions.insert(id, tx.clone());

        if sender {
            log::info!("connect: Adding sender connection client_id={client_id} conn_id={id}");
            upsert_connection(&client_id, &id.to_string()).await?;
            CACHE_CONNECTIONS_MAP.write().unwrap().insert(client_id, id);
        } else {
            log::info!("connect: Adding client connection client_id={client_id} conn_id={id}");
            self.clients.insert(id, tx.clone());
        }

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst) + 1;
        log::debug!("connect: Visitor count: {count}");

        // send id back
        Ok(id)
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    async fn disconnect(&mut self, conn_id: ConnId) -> Result<(), DatabaseError> {
        log::debug!("disconnect: Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst) - 1;
        log::debug!("disconnect: Visitor count: {count}");

        delete_connection(&conn_id.to_string()).await?;

        CACHE_CONNECTIONS_MAP
            .write()
            .unwrap()
            .retain(|client_id, id| {
                if *id == conn_id {
                    log::info!(
                        "disconnect: Removed sender connection client_id={client_id} conn_id={id}"
                    );
                    false
                } else {
                    log::trace!(
                        "disconnect: Retained sender connection client_id={client_id} conn_id={id}"
                    );
                    true
                }
            });

        // remove sender
        if self.sessions.remove(&conn_id).is_some() {
            log::debug!("disconnect: Removed client session conn_id={conn_id}");
        };
        if self.clients.remove(&conn_id).is_some() {
            log::info!("disconnect: Removed client connection conn_id={conn_id}");
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WsRequestError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

#[derive(Error, Debug)]
pub enum ConnectionIdError {
    #[error("Invalid Connection ID '{0}'")]
    Invalid(String),
    #[error("Connection ID not found for client_id '{0}'")]
    NotFound(String),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

static CACHE_CONNECTIONS_MAP: LazyLock<std::sync::RwLock<HashMap<String, usize>>> =
    LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

impl service::Handle {
    /// Register client message sender and obtain connection ID.
    pub async fn connect(
        &self,
        client_id: &str,
        sender: bool,
        conn_tx: mpsc::UnboundedSender<String>,
    ) -> Result<ConnId, CommanderError> {
        let (res_tx, res_rx) = oneshot::channel();
        // unwrap: ws server should not have been dropped
        self.send_command_async(Command::Connect {
            conn_tx,
            res_tx,
            client_id: client_id.to_string(),
            sender,
        })
        .await?;

        Ok(res_rx.await.unwrap())
    }

    pub async fn ws_request(
        &self,
        conn_id: usize,
        client_id: &str,
        profile: Option<String>,
        msg: impl Into<String>,
    ) -> Result<(), WsRequestError> {
        let request_id = thread_rng().gen::<usize>();

        self.send_command_async(Command::WsRequest {
            request_id,
            conn_id,
            client_id: client_id.to_string(),
            body: msg.into(),
            profile,
        })
        .await
        .unwrap();
        Ok(())
    }

    pub async fn ws_message(&self, message: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.send_command_async(Command::WsMessage { message })
            .await
            .unwrap();

        Ok(())
    }

    pub async fn ws_response(&self, response: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.send_command_async(Command::WsResponse { response })
            .await
            .unwrap();

        Ok(())
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub async fn disconnect(&self, conn: ConnId) {
        // unwrap: ws server should not have been dropped
        self.send_command_async(Command::Disconnect { conn })
            .await
            .unwrap();
    }

    pub async fn response(&self, conn_id: ConnId, response: TunnelResponse) {
        self.send_command_async(Command::Response { conn_id, response })
            .await
            .unwrap();
    }
}

pub async fn get_connection_id(client_id: &str) -> Result<usize, ConnectionIdError> {
    let existing = {
        let lock = CACHE_CONNECTIONS_MAP.read().unwrap();
        lock.get(client_id).copied()
    };
    if let Some(conn_id) = existing {
        Ok(conn_id)
    } else {
        let tunnel_ws_id = select_connection(client_id)
            .await?
            .ok_or(ConnectionIdError::NotFound(client_id.to_string()))?
            .tunnel_ws_id;

        let conn_id = tunnel_ws_id
            .parse::<usize>()
            .map_err(|_| ConnectionIdError::Invalid(tunnel_ws_id))?;

        CACHE_CONNECTIONS_MAP
            .write()
            .unwrap()
            .insert(client_id.to_string(), conn_id);

        Ok(conn_id)
    }
}
