//! WebSocket server implementation for managing tunnel connections.
//!
//! This module provides the core WebSocket server that manages client connections,
//! routes HTTP requests through tunnels, handles WebSocket message passing, and
//! coordinates connection lifecycle. It uses an async command-based architecture
//! for thread-safe operation.

#![allow(clippy::module_name_repetitions)]

use std::{
    collections::BTreeMap,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use moosicbox_tunnel::{
    TunnelAbortRequest, TunnelRequest, TunnelResponse, TunnelWsRequest, TunnelWsResponse,
};
use moosicbox_tunnel_server::CANCELLATION_TOKEN;
use serde_json::Value;
use strum_macros::AsRefStr;
use switchy_async::sync::{
    RwLock,
    mpsc::{SendError, Sender as UnboundedSender},
    oneshot,
};
use switchy_async::util::CancellationToken;
use thiserror::Error;

use crate::db::{DatabaseError, delete_connection, select_connection, upsert_connection};
use crate::ws::{ConnId, Msg};

use self::service::{Commander, CommanderError};

/// A command received by the [`WsServer`].
///
/// Commands are used to communicate with the WebSocket server asynchronously.
/// Each variant represents a different operation that can be performed on the server.
#[derive(Debug, AsRefStr)]
pub enum Command {
    /// Establish a new WebSocket connection.
    Connect {
        /// Channel sender for messages to this connection.
        conn_tx: UnboundedSender<Msg>,
        /// Response channel to send back the connection ID.
        res_tx: oneshot::Sender<ConnId>,
        /// The unique identifier for the client.
        client_id: String,
        /// Whether this connection is acting as a sender.
        sender: bool,
    },

    /// Close an existing WebSocket connection.
    Disconnect {
        /// The connection ID to disconnect.
        conn: ConnId,
    },

    /// Start tracking a new HTTP request being tunneled.
    RequestStart {
        /// The unique identifier for this request.
        request_id: u64,
        /// Channel to send response data chunks.
        sender: UnboundedSender<TunnelResponse>,
        /// Channel to send response headers.
        headers_sender: oneshot::Sender<RequestHeaders>,
        /// Token used to abort the request if needed.
        abort_request_token: CancellationToken,
    },

    /// Mark an HTTP request as completed and clean up resources.
    RequestEnd {
        /// The unique identifier for the request that ended.
        request_id: u64,
    },

    /// Receive a response from a tunneled HTTP request.
    Response {
        /// The response data from the client.
        response: TunnelResponse,
        /// The connection ID that sent this response.
        conn_id: ConnId,
    },

    /// Send a WebSocket request to a client.
    WsRequest {
        /// The unique identifier for this WebSocket request.
        request_id: u64,
        /// The connection ID that initiated this request.
        conn_id: ConnId,
        /// The unique identifier for the target client.
        client_id: String,
        /// The request body as a JSON string.
        body: String,
        /// Optional profile identifier for request routing.
        profile: Option<String>,
    },

    /// Broadcast or send a WebSocket message to clients.
    WsMessage {
        /// The WebSocket message to send.
        message: TunnelWsResponse,
    },

    /// Send a WebSocket response back to the originating connection.
    WsResponse {
        /// The WebSocket response to send.
        response: TunnelWsResponse,
    },

    /// Send a direct message to a specific connection.
    Message {
        /// The message content to send.
        msg: Msg,
        /// The target connection ID.
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

    #[allow(clippy::too_many_lines)]
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
                    log::debug!(
                        "process_command: Response request_id={request_id} status={status} headers={headers:?}"
                    );
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
                            let response = ctx.read().await.abort_request(conn_id, request_id);
                            if let Err(err) = response {
                                log::error!(
                                    "process_command: Failed to abort request_id={request_id}: {err:?}"
                                );
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
                    log::trace!(
                        "process_command: Sending response for request_id={request_id} packet_id={packet_id} last={last} status={status:?}"
                    );
                    if sender.send(response).is_err() {
                        log::debug!("process_command: Sender dropped for request_id={request_id}");
                        let mut binding = ctx.write().await;
                        binding.senders.remove(&request_id);
                        drop(binding);
                        let response = ctx.read().await.abort_request(conn_id, request_id);
                        if let Err(err) = response {
                            log::error!(
                                "process_command: Failed to abort request_id={request_id} {err:?}"
                            );
                        }
                    } else {
                        log::trace!(
                            "process_command: Sent response for request_id={request_id} packet_id={packet_id} last={last} status={status:?}"
                        );
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
                        .send_message_to(client_conn_id, serde_json::to_string(&body).unwrap());
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
                        let response = binding.send_message_to(conn_id, message.body.to_string());
                        drop(binding);
                        if let Err(error) = response {
                            log::error!("Failed to send WsResponse to {conn_id}: {error:?}");
                        }
                    }
                } else if let Some(exclude_connection_ids) = message.exclude_connection_ids {
                    let binding = ctx.read().await;
                    let response =
                        binding.broadcast_except(&exclude_connection_ids, message.body.to_string());
                    drop(binding);
                    if let Err(error) = response {
                        log::error!("Failed to broadcast_except WsMessage: {error:?}");
                    }
                } else {
                    let binding = ctx.read().await;
                    let response = binding.broadcast(message.body.to_string());
                    drop(binding);
                    if let Err(error) = response {
                        log::error!("Failed to broadcast WsMessage: {error:?}");
                    }
                }
            }

            Command::WsResponse { response } => {
                let binding = ctx.read().await;
                let ws_id = binding.ws_requests.get(&response.request_id).copied();
                drop(binding);
                if let Some(ws_id) = ws_id {
                    let binding = ctx.read().await;
                    let response = binding.send_message_to(ws_id, response.body.to_string());
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
                let response = binding.send_message_to(conn, &msg);
                drop(binding);
                if let Err(error) = response {
                    log::error!("Failed to send message to {conn}: {msg:?}: {error:?}");
                }
            }
        }
        Ok(())
    }
}

/// HTTP response headers from a tunneled request.
///
/// Contains the status code and headers that were received from the client
/// handling the tunneled HTTP request.
#[derive(Debug)]
pub struct RequestHeaders {
    /// HTTP status code of the response.
    pub status: u16,
    /// HTTP headers of the response as key-value pairs.
    pub headers: BTreeMap<String, String>,
}

/// WebSocket server managing tunnel connections and HTTP request proxying.
///
/// The server maintains active WebSocket connections from clients and routes HTTP
/// requests through those connections. It handles both sender connections (which
/// respond to HTTP requests) and client connections (which can initiate WebSocket
/// requests).
#[derive(Debug)]
pub struct WsServer {
    /// Map of all connection IDs to their message senders (both senders and clients).
    sessions: BTreeMap<ConnId, UnboundedSender<Msg>>,
    /// Map of client connection IDs to their message senders.
    clients: BTreeMap<ConnId, UnboundedSender<Msg>>,
    /// Map of request IDs to their response data senders.
    senders: BTreeMap<u64, UnboundedSender<TunnelResponse>>,
    /// Map of request IDs to their response headers senders.
    headers_senders: BTreeMap<u64, oneshot::Sender<RequestHeaders>>,
    /// Map of request IDs to their abort tokens for cancellation.
    abort_request_tokens: BTreeMap<u64, CancellationToken>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Map of WebSocket request IDs to the connection IDs that initiated them.
    ws_requests: BTreeMap<u64, ConnId>,
}

/// Errors that can occur when sending WebSocket messages.
#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    /// The specified connection ID is not currently connected.
    #[error("Session {0} not connected")]
    NoSession(ConnId),
    /// Failed to send a message through the WebSocket channel.
    #[error(transparent)]
    WebsocketSend(#[from] SendError<String>),
}

impl WsServer {
    /// Create a new WebSocket server instance.
    ///
    /// Initializes an empty server with no active connections. The server is ready
    /// to accept WebSocket connections and route HTTP requests through tunnel connections.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            clients: BTreeMap::new(),
            senders: BTreeMap::new(),
            headers_senders: BTreeMap::new(),
            abort_request_tokens: BTreeMap::new(),
            visitor_count: Arc::new(AtomicUsize::new(0)),
            ws_requests: BTreeMap::new(),
        }
    }

    fn abort_request(&self, id: ConnId, request_id: u64) -> Result<(), WebsocketMessageError> {
        log::debug!("Aborting request {request_id} (conn_id={id})");
        if let Some(abort_token) = self.abort_request_tokens.get(&request_id) {
            abort_token.cancel();
        } else {
            log::debug!("No abort token for request {request_id}");
        }
        let body = TunnelRequest::Abort(TunnelAbortRequest { request_id });
        self.send_message_to(id, serde_json::to_string(&body).unwrap())
    }

    /// Send message directly to the user.
    fn send_message_to(
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
    fn broadcast(&self, msg: impl Into<String>) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message");
        let message = msg.into();

        for session in self.clients.values() {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?;
        }
        Ok(())
    }

    /// Send message directly to the user.
    fn broadcast_except(
        &self,
        ids: &[ConnId],
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message except {ids:?}");
        let message = msg.into();

        for (id, session) in &self.clients {
            if ids.contains(id) {
                continue;
            }
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?;
        }
        Ok(())
    }

    /// Register new session and assign unique ID to this session
    async fn connect(
        &mut self,
        client_id: String,
        sender: bool,
        tx: UnboundedSender<Msg>,
    ) -> Result<ConnId, DatabaseError> {
        // register session with random connection ID
        let id = switchy_random::rng().next_u64();

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
        }
        if self.clients.remove(&conn_id).is_some() {
            log::info!("disconnect: Removed client connection conn_id={conn_id}");
        }

        Ok(())
    }
}

/// Errors that can occur when processing WebSocket requests.
#[derive(Debug, Error)]
pub enum WsRequestError {
    /// Database operation failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Errors that can occur when looking up a connection ID.
#[derive(Error, Debug)]
pub enum ConnectionIdError {
    /// The connection ID string could not be parsed.
    #[error("Invalid Connection ID '{0}'")]
    Invalid(String),
    /// No connection was found for the specified client ID.
    #[error("Connection ID not found for client_id '{0}'")]
    NotFound(String),
    /// Database operation failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

static CACHE_CONNECTIONS_MAP: LazyLock<std::sync::RwLock<BTreeMap<String, ConnId>>> =
    LazyLock::new(|| std::sync::RwLock::new(BTreeMap::new()));

impl service::Handle {
    /// Register client message sender and obtain connection ID.
    ///
    /// Establishes a new WebSocket connection with the server and returns a unique
    /// connection ID. The connection can be either a sender (handling HTTP requests)
    /// or a client (initiating WebSocket requests).
    ///
    /// # Errors
    ///
    /// * Returns [`CommanderError`] if the server command channel is closed or the server panicked.
    ///
    /// # Panics
    ///
    /// * Panics if the connection response channel is dropped before receiving the connection ID.
    pub async fn connect(
        &self,
        client_id: &str,
        sender: bool,
        conn_tx: UnboundedSender<String>,
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

    /// Send a WebSocket request to a client connection.
    ///
    /// Routes a WebSocket request from one connection to another client's connection.
    /// The request is assigned a unique request ID for tracking the response.
    ///
    /// # Errors
    ///
    /// * Returns [`WsRequestError`] if the database operation fails.
    ///
    /// # Panics
    ///
    /// * Panics if sending the command to the server fails.
    pub async fn ws_request(
        &self,
        conn_id: ConnId,
        client_id: &str,
        profile: Option<String>,
        msg: impl Into<String> + Send,
    ) -> Result<(), WsRequestError> {
        let request_id = switchy_random::rng().next_u64();

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

    /// Broadcast or send a WebSocket message to client connections.
    ///
    /// Sends a WebSocket message to specific connections (via `to_connection_ids`),
    /// broadcasts to all except specific connections (via `exclude_connection_ids`),
    /// or broadcasts to all client connections.
    ///
    /// # Errors
    ///
    /// * Returns [`WsRequestError`] if the database operation fails.
    ///
    /// # Panics
    ///
    /// * Panics if sending the command to the server fails.
    pub async fn ws_message(&self, message: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.send_command_async(Command::WsMessage { message })
            .await
            .unwrap();

        Ok(())
    }

    /// Send a WebSocket response back to the originating connection.
    ///
    /// Routes a WebSocket response back to the connection that initiated the request,
    /// identified by the request ID in the response.
    ///
    /// # Errors
    ///
    /// * Returns [`WsRequestError`] if the database operation fails.
    ///
    /// # Panics
    ///
    /// * Panics if sending the command to the server fails.
    pub async fn ws_response(&self, response: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.send_command_async(Command::WsResponse { response })
            .await
            .unwrap();

        Ok(())
    }

    /// Unregister message sender and clean up connection resources.
    ///
    /// Removes the connection from the server and cleans up all associated state
    /// including database records and in-memory caches.
    ///
    /// # Panics
    ///
    /// * Panics if sending the disconnect command to the server fails.
    pub async fn disconnect(&self, conn: ConnId) {
        // unwrap: ws server should not have been dropped
        self.send_command_async(Command::Disconnect { conn })
            .await
            .unwrap();
    }

    /// Send an HTTP tunnel response to the request handler.
    ///
    /// Routes a response chunk from the client back to the HTTP request handler
    /// that is waiting for the response. This is used for streaming HTTP responses
    /// through the WebSocket tunnel.
    ///
    /// # Panics
    ///
    /// * Panics if sending the response command to the server fails.
    pub async fn response(&self, conn_id: ConnId, response: TunnelResponse) {
        self.send_command_async(Command::Response { conn_id, response })
            .await
            .unwrap();
    }
}

/// Look up the connection ID for a given client ID.
///
/// This function first checks an in-memory cache, and if not found, queries the
/// database to find the connection ID associated with the client ID.
///
/// # Errors
///
/// * [`ConnectionIdError::NotFound`] - No connection exists for the given client ID.
/// * [`ConnectionIdError::Invalid`] - The stored connection ID could not be parsed.
/// * [`ConnectionIdError::Database`] - Database query failed.
///
/// # Panics
///
/// Panics if the connection cache lock is poisoned.
pub async fn get_connection_id(client_id: &str) -> Result<ConnId, ConnectionIdError> {
    let existing = {
        let lock = CACHE_CONNECTIONS_MAP.read().unwrap();
        lock.get(client_id).copied()
    };
    if let Some(conn_id) = existing {
        Ok(conn_id)
    } else {
        let tunnel_ws_id = select_connection(client_id)
            .await?
            .ok_or_else(|| ConnectionIdError::NotFound(client_id.to_string()))?
            .tunnel_ws_id;

        let conn_id = tunnel_ws_id
            .parse::<ConnId>()
            .map_err(|_| ConnectionIdError::Invalid(tunnel_ws_id))?;

        CACHE_CONNECTIONS_MAP
            .write()
            .unwrap()
            .insert(client_id.to_string(), conn_id);

        Ok(conn_id)
    }
}
