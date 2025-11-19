//! Core tunnel sender implementation for managing WebSocket connections.
//!
//! This module provides the main [`TunnelSender`] type for establishing and maintaining
//! tunnel connections, along with [`TunnelSenderHandle`] for controlling active connections.
//! It handles HTTP request proxying, WebSocket message routing, and bidirectional
//! communication through tunnel connections.

#![allow(clippy::module_name_repetitions)]

use std::{
    collections::BTreeMap,
    fs::File,
    io::Cursor,
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use async_trait::async_trait;
#[cfg(feature = "base64")]
use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use futures_util::{
    Future, Stream, StreamExt,
    future::{self, ready},
    pin_mut,
};
use moosicbox_audio_decoder::{
    AudioDecodeHandler, media_sources::remote_bytestream::RemoteByteStreamMediaSource,
};
use moosicbox_auth::AuthError;
use moosicbox_channel_utils::{MoosicBoxSender as _, futures_channel::PrioritizedSender};
use moosicbox_env_utils::default_env_usize;
use moosicbox_files::{
    api::AlbumCoverQuery,
    files::{
        album::{AlbumCoverError, get_album_cover},
        track::{audio_format_to_content_type, get_track_id_source, get_track_info},
    },
    range::{Range, parse_ranges},
};
use moosicbox_music_api::{SourceToMusicApi as _, models::TrackSource};
use moosicbox_music_models::{ApiSource, AudioFormat, id::Id};
use moosicbox_player::symphonia::play_media_source_async;
use moosicbox_stream_utils::{ByteWriter, remote_bytestream::RemoteByteStream};
use moosicbox_tunnel::{TunnelEncoding, TunnelWsResponse};
use moosicbox_ws::{PlayerAction, WebsocketContext, WebsocketSendError, WebsocketSender};
use regex::Regex;
use serde_json::Value;
use switchy_async::util::CancellationToken;
use switchy_database::{config::ConfigDatabase, profiles::PROFILES};
use switchy_http::models::Method;
use symphonia::core::{
    io::{MediaSourceStream, MediaSourceStreamOptions},
    probe::Hint,
};
use thiserror::Error;
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender, channel, error::SendError},
    time::sleep,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error, Message, Utf8Bytes},
};

use super::{
    GetTrackInfoQuery, GetTrackQuery, SendBytesError, SendMessageError, TunnelMessage,
    TunnelRequestError,
};
use crate::websocket_sender::TunnelWebsocketSender;

/// Error type for closing tunnel connections.
#[derive(Debug, Error)]
pub enum CloseError {
    /// Unknown error occurred during connection close.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Handle for controlling and interacting with a tunnel sender.
///
/// Provides methods to close the connection and add player actions.
#[derive(Clone)]
pub struct TunnelSenderHandle {
    #[allow(clippy::type_complexity)]
    sender: Arc<RwLock<Option<PrioritizedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
    player_actions: Arc<RwLock<Vec<(u64, PlayerAction)>>>,
}

impl TunnelSenderHandle {
    /// Closes the tunnel connection by cancelling the cancellation token.
    pub fn close(&self) {
        self.cancellation_token.cancel();
    }

    /// Adds a player action to the tunnel sender.
    ///
    /// # Panics
    ///
    /// * If the `player_actions` `RwLock` is poisoned
    pub fn add_player_action(&self, id: u64, action: PlayerAction) {
        self.player_actions.write().unwrap().push((id, action));
    }
}

#[allow(unused)]
fn wrap_to_500<E: std::error::Error + Send + 'static>(e: E) -> TunnelRequestError {
    TunnelRequestError::InternalServerError(Box::new(e))
}

#[async_trait]
impl WebsocketSender for TunnelSenderHandle {
    /// Sends a message to a specific connection through the tunnel.
    ///
    /// # Panics
    ///
    /// * If the `sender` `RwLock` is poisoned
    async fn send(
        &self,
        conn_id: &str,
        data: &str,
    ) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: None,
                    to_connection_ids: Some(vec![conn_id.parse::<u64>()?]),
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    /// Sends a message to all connections through the tunnel.
    ///
    /// # Panics
    ///
    /// * If the `sender` `RwLock` is poisoned
    async fn send_all(&self, data: &str) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: None,
                    to_connection_ids: None,
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    /// Sends a message to all connections except the specified one through the tunnel.
    ///
    /// # Panics
    ///
    /// * If the `sender` `RwLock` is poisoned
    async fn send_all_except(
        &self,
        conn_id: &str,
        data: &str,
    ) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: Some(vec![conn_id.parse::<u64>()?]),
                    to_connection_ids: None,
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    /// Sends a ping control message through the tunnel.
    ///
    /// # Panics
    ///
    /// * If the `sender` `RwLock` is poisoned
    async fn ping(&self) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Ping)
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }
}

/// Message types that can be sent through the tunnel.
pub enum TunnelResponseMessage {
    /// A packet response for a specific tunnel request.
    Packet(TunnelResponsePacket),
    /// A WebSocket message with connection filtering.
    Ws(TunnelResponseWs),
    /// A ping control message.
    Ping,
}

/// A packet response for a specific tunnel request.
pub struct TunnelResponsePacket {
    /// The unique identifier for the tunnel request.
    pub request_id: u64,
    /// The sequence number of this packet in the response stream.
    pub packet_id: u32,
    /// The WebSocket message to send.
    pub message: Message,
    /// Whether to broadcast this packet to all connections.
    pub broadcast: bool,
    /// Optional connection ID to exclude from broadcast.
    pub except_id: Option<u64>,
    /// Optional connection ID to send exclusively to.
    pub only_id: Option<u64>,
}

/// A websocket response with connection filtering options.
pub struct TunnelResponseWs {
    /// The WebSocket message to send.
    pub message: Message,
    /// Connection IDs to exclude from receiving this message.
    pub exclude_connection_ids: Option<Vec<u64>>,
    /// Connection IDs that should exclusively receive this message.
    pub to_connection_ids: Option<Vec<u64>>,
}

/// Main tunnel sender that manages websocket connections and request forwarding.
///
/// Handles communication between local services and remote tunnel servers,
/// processing HTTP requests and websocket messages.
#[derive(Clone)]
pub struct TunnelSender {
    id: u64,
    host: String,
    url: String,
    client_id: String,
    access_token: String,
    sender: Arc<RwLock<Option<PrioritizedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
    abort_request_tokens: Arc<RwLock<BTreeMap<u64, CancellationToken>>>,
    player_actions: Arc<RwLock<Vec<(u64, PlayerAction)>>>,
    config_db: ConfigDatabase,
}

static BINARY_REQUEST_BUFFER_OFFSET: LazyLock<usize> = LazyLock::new(|| {
    std::mem::size_of::<usize>() + // request_id
    std::mem::size_of::<u32>() + // packet_id
    std::mem::size_of::<u8>() // last
});

static DEFAULT_WS_MAX_PACKET_SIZE: usize = 1024 * 64;
static WS_MAX_PACKET_SIZE: usize =
    default_env_usize!("WS_MAX_PACKET_SIZE", DEFAULT_WS_MAX_PACKET_SIZE);

impl TunnelSender {
    /// Creates a new tunnel sender with the specified connection parameters.
    ///
    /// Returns both the sender and a handle for controlling it.
    #[must_use]
    pub fn new(
        host: String,
        url: String,
        client_id: String,
        access_token: String,
        config_db: ConfigDatabase,
    ) -> (Self, TunnelSenderHandle) {
        let sender = Arc::new(RwLock::new(None));
        let cancellation_token = CancellationToken::new();
        let id = switchy_random::rng().next_u64();
        let player_actions = Arc::new(RwLock::new(vec![]));
        let handle = TunnelSenderHandle {
            sender: sender.clone(),
            cancellation_token: cancellation_token.clone(),
            player_actions: player_actions.clone(),
        };

        (
            Self {
                id,
                host,
                url,
                client_id,
                access_token,
                sender,
                cancellation_token,
                abort_request_tokens: Arc::new(RwLock::new(BTreeMap::new())),
                player_actions,
                config_db,
            },
            handle,
        )
    }

    /// Sets a custom cancellation token for the tunnel sender.
    ///
    /// Replaces the default cancellation token with the provided one, allowing
    /// external control over the tunnel lifecycle.
    #[must_use]
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    /// Adds a player action to be tracked by the tunnel sender.
    ///
    /// # Panics
    ///
    /// * If the `player_actions` `RwLock` is poisoned
    #[must_use]
    pub fn add_player_action(self, id: u64, action: PlayerAction) -> Self {
        self.player_actions.write().unwrap().push((id, action));
        self
    }

    async fn message_handler(
        tx: Sender<TunnelMessage>,
        m: Message,
    ) -> Result<(), SendError<TunnelMessage>> {
        log::trace!("Message from tunnel ws server: {m:?}");
        tx.send(match m {
            Message::Text(m) => TunnelMessage::Text(m.to_string()),
            Message::Binary(m) => TunnelMessage::Binary(m),
            Message::Ping(m) => TunnelMessage::Ping(m.to_vec()),
            Message::Pong(m) => TunnelMessage::Pong(m.to_vec()),
            Message::Close(_m) => TunnelMessage::Close,
            Message::Frame(m) => TunnelMessage::Frame(m),
        })
        .await
    }

    /// Starts the tunnel connection and returns a receiver for incoming messages.
    ///
    /// Initiates the WebSocket connection to the tunnel server and returns a channel
    /// receiver for processing incoming tunnel messages.
    #[must_use]
    pub fn start(&self) -> Receiver<TunnelMessage> {
        self.start_tunnel(Self::message_handler)
    }

    fn is_request_aborted(
        request_id: u64,
        tokens: &Arc<RwLock<BTreeMap<u64, CancellationToken>>>,
    ) -> bool {
        if let Some(token) = tokens.read().unwrap().get(&request_id) {
            return token.is_cancelled();
        }
        false
    }

    #[allow(clippy::too_many_lines)]
    fn start_tunnel<T, O>(&self, handler: fn(sender: Sender<T>, m: Message) -> O) -> Receiver<T>
    where
        T: Send + 'static,
        O: Future<Output = Result<(), SendError<T>>> + Send + 'static,
    {
        let (tx, rx) = channel(1024);

        let host = self.host.clone();
        let url = self.url.clone();
        let client_id = self.client_id.clone();
        let access_token = self.access_token.clone();
        let sender_arc = self.sender.clone();
        let abort_request_tokens = self.abort_request_tokens.clone();
        let cancellation_token = self.cancellation_token.clone();

        switchy_async::runtime::Handle::current().spawn_with_name("tunnel_sender", async move {
            let mut just_retried = false;
            log::debug!("Fetching signature token...");
            let token = loop {
                match select!(
                    resp = moosicbox_auth::fetch_signature_token(&host, &client_id, &access_token) => resp,
                    () = cancellation_token.cancelled() => {
                        log::debug!("Cancelling fetch");
                        return;
                    }
                ) {
                    Ok(Some(token)) => break token,
                    Ok(None) => {
                        log::error!("Failed to fetch token, no response");
                    }
                    Err(AuthError::Unauthorized) => {
                        log::error!("Unauthorized response from fetch_signature_token");
                    }
                    Err(err) => {
                        log::error!("Failed to fetch signature token: {err:?}");
                    }
                }

                select!(
                    () = sleep(Duration::from_millis(5000)) => {}
                    () = cancellation_token.cancelled() => {
                        log::debug!("Cancelling retry");
                        return;
                    }
                );
            };

            loop {
                let close_token = CancellationToken::new();

                let (txf, rxf) = moosicbox_channel_utils::futures_channel::unbounded();
                let txf = txf.with_priority(|message: &TunnelResponseMessage| match message {
                    TunnelResponseMessage::Packet(packet) => {
                        log::debug!(
                            "determining priority for packet: packet_id={} len={}",
                            packet.packet_id,
                            packet.message.len()
                        );
                        usize::MAX - packet.message.len()
                    }
                    TunnelResponseMessage::Ws(ws) => {
                        log::debug!("determining priority for ws: len={}", ws.message.len());
                        usize::MAX - ws.message.len()
                    }
                    TunnelResponseMessage::Ping => {
                        log::debug!("determining priority for ping");
                        usize::MAX
                    }
                });

                sender_arc.write().unwrap().replace(txf.clone());

                log::debug!("Connecting to websocket...");

                match select!(
                    resp = connect_async(
                        format!("{url}?clientId={client_id}&sender=true&signature={token}"),
                    ) => resp,
                    () = cancellation_token.cancelled() => {
                        log::debug!("Cancelling connect");
                        break;
                    }
                ) {
                    Ok((ws_stream, _)) => {
                        log::debug!("WebSocket handshake has been successfully completed");

                        if just_retried {
                            log::info!("WebSocket successfully reconnected");
                            just_retried = false;
                        }

                        let (write, read) = ws_stream.split();

                        let ws_writer = rxf
                                .filter(|message| {
                                    match message {
                                        TunnelResponseMessage::Packet(packet) => {
                                            if Self::is_request_aborted(packet.request_id, &abort_request_tokens) {
                                                log::debug!(
                                                    "Not sending packet from aborted request request_id={} packet_id={} size={}",
                                                    packet.request_id,
                                                    packet.packet_id,
                                                    packet.message.len()
                                                );
                                                return ready(false);
                                            }
                                        },
                                        TunnelResponseMessage::Ws(_ws) => {}
                                        TunnelResponseMessage::Ping => {}
                                    }

                                    ready(true)
                                })
                                .map(|message| {
                                    match message {
                                        TunnelResponseMessage::Packet(packet) => {
                                            log::debug!(
                                                "Sending packet from request request_id={} packet_id={} size={}",
                                                packet.request_id,
                                                packet.packet_id,
                                                packet.message.len()
                                            );
                                            Ok(packet.message)
                                        },
                                        TunnelResponseMessage::Ws(ws) => {
                                            if let Message::Text(text) = ws.message {
                                                if log::log_enabled!(log::Level::Trace) {
                                                    log::debug!(
                                                        "Sending ws message to={:?} exclude={:?} size={} message={}",
                                                        ws.to_connection_ids,
                                                        ws.exclude_connection_ids,
                                                        text.len(),
                                                        text,
                                                    );
                                                } else {
                                                    log::debug!(
                                                        "Sending ws message to={:?} exclude={:?} size={}",
                                                        ws.to_connection_ids,
                                                        ws.exclude_connection_ids,
                                                        text.len(),
                                                    );
                                                }
                                                serde_json::from_str(text.as_str()).and_then(|value: Value| {
                                                    serde_json::to_string(&TunnelWsResponse {
                                                        request_id: 0,
                                                        body: value,
                                                        exclude_connection_ids: ws.exclude_connection_ids,
                                                        to_connection_ids: ws.to_connection_ids,
                                                    })
                                                        .map(Utf8Bytes::from)
                                                        .map(Message::Text)
                                                }).map_err(|e| {
                                                    log::error!("Serde error occurred: {e:?}");
                                                    tokio_tungstenite::tungstenite::Error::AlreadyClosed
                                                })
                                            } else {
                                                Ok(ws.message)
                                            }
                                        },
                                        TunnelResponseMessage::Ping => {
                                            log::trace!("Sending ping");
                                            Ok(Message::Ping(Bytes::new()))
                                        }
                                    }
                                })
                                .forward(write);

                        let ws_reader = read.for_each(|m| async {
                            let m = match m {
                                Ok(m) => m,
                                Err(e) => {
                                    log::error!("Send Loop error: {e:?}");
                                    close_token.cancel();
                                    return;
                                }
                            };

                            switchy_async::runtime::Handle::current().spawn_with_name("tunnel_sender: Process WS message", {
                                let tx = tx.clone();
                                let close_token = close_token.clone();

                                async move {
                                    if let Err(e) = handler(tx.clone(), m).await {
                                        log::error!("Handler Send Loop error: {e:?}");
                                        close_token.cancel();
                                    }
                                }
                            });
                        });

                        let pinger = switchy_async::runtime::Handle::current().spawn_with_name("tunnel_sender: pinger", {
                            let txf = txf.clone();
                            let close_token = close_token.clone();
                            let cancellation_token = cancellation_token.clone();

                            async move {
                                loop {
                                    select!(
                                        () = close_token.cancelled() => { break; }
                                        () = cancellation_token.cancelled() => { break; }
                                        () = tokio::time::sleep(std::time::Duration::from_millis(5000)) => {
                                            log::trace!("Sending ping to tunnel");
                                            if let Err(e) = txf.send(TunnelResponseMessage::Ping) {
                                                log::error!("Pinger Send Loop error: {e:?}");
                                                close_token.cancel();
                                                break;
                                            }
                                        }
                                    );
                                }
                            }
                        });

                        pin_mut!(ws_writer, ws_reader);
                        select!(
                            () = close_token.cancelled() => {}
                            () = cancellation_token.cancelled() => {}
                            _ = future::select(ws_writer, ws_reader) => {}
                        );
                        if !close_token.is_cancelled() {
                            close_token.cancel();
                        }
                        log::debug!("start_tunnel: Waiting for pinger to finish...");
                        if let Err(e) = pinger.await {
                            log::warn!("start_tunnel: Pinger failed to finish: {e:?}");
                        }
                        log::info!("WebSocket connection closed");
                    }
                    Err(err) => {
                        if let Error::Http(response) = err {
                            if let Ok(body) =
                                std::str::from_utf8(response.body().as_ref().unwrap_or(&vec![]))
                            {
                                log::error!("error ({}): {body}", response.status());
                            } else {
                                log::error!("body: (unable to get body)");
                            }
                        } else {
                            log::error!("Failed to connect to websocket server: {err:?}");
                        }
                    }
                }

                if just_retried {
                    select!(
                        () = sleep(Duration::from_millis(5000)) => {}
                        () = cancellation_token.cancelled() => {
                            log::debug!("Cancelling retry");
                            break;
                        }
                    );
                } else {
                    just_retried = true;
                }
            }

            log::debug!("Tunnel closed");
        });

        rx
    }

    /// Sends raw bytes through the tunnel for a specific request.
    ///
    /// # Panics
    ///
    /// * If the `Sender` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to send the bytes
    pub fn send_bytes(
        &self,
        request_id: u64,
        packet_id: u32,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), SendBytesError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Packet(TunnelResponsePacket {
                    request_id,
                    packet_id,
                    broadcast: true,
                    except_id: None,
                    only_id: None,
                    message: Message::Binary(Bytes::from(bytes.into())),
                }))
                .map_err(|err| SendBytesError::Unknown(format!("Failed to send_bytes: {err:?}")))?;
        } else {
            return Err(SendBytesError::Unknown(
                "Failed to get sender for send_bytes".into(),
            ));
        }

        Ok(())
    }

    /// Sends a text message through the tunnel for a specific request.
    ///
    /// # Panics
    ///
    /// * If the `Sender` `RwLock` is poisoned
    ///
    /// # Errors
    ///
    /// * If failed to send the message
    pub fn send_message(
        &self,
        request_id: u64,
        packet_id: u32,
        message: impl Into<String>,
    ) -> Result<(), SendMessageError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .send(TunnelResponseMessage::Packet(TunnelResponsePacket {
                    request_id,
                    packet_id,
                    broadcast: true,
                    except_id: None,
                    only_id: None,
                    message: Message::Text(Utf8Bytes::from(message.into())),
                }))
                .map_err(|err| {
                    SendMessageError::Unknown(format!("Failed to send_message: {err:?}"))
                })?;
        } else {
            return Err(SendMessageError::Unknown(
                "Failed to get sender for send_message".into(),
            ));
        }

        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn send(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        reader: impl std::io::Read,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        match encoding {
            TunnelEncoding::Binary => {
                self.send_binary(request_id, status, headers, reader);
                Ok(())
            }
            #[cfg(feature = "base64")]
            TunnelEncoding::Base64 => self.send_base64(request_id, status, headers, reader),
        }
    }

    async fn send_stream<E: std::error::Error + Sized>(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        ranges: Option<Vec<Range>>,
        stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin + Send,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        match encoding {
            TunnelEncoding::Binary => {
                self.send_binary_stream(request_id, status, headers, ranges, stream)
                    .await
            }
            #[cfg(feature = "base64")]
            TunnelEncoding::Base64 => {
                self.send_base64_stream(request_id, status, headers, ranges, stream)
                    .await
            }
        }
    }

    fn init_binary_request_buffer(
        request_id: u64,
        packet_id: u32,
        last: bool,
        status: u16,
        headers: &BTreeMap<String, String>,
        buf: &mut [u8],
    ) -> usize {
        let mut offset = 0_usize;

        let id_bytes = request_id.to_be_bytes();
        let len = id_bytes.len();
        buf[..len].copy_from_slice(&id_bytes);
        offset += len;

        let packet_id_bytes = packet_id.to_be_bytes();
        let len = packet_id_bytes.len();
        buf[offset..(offset + len)].copy_from_slice(&packet_id_bytes);
        offset += len;

        let last_bytes = u8::from(last).to_be_bytes();
        let len = last_bytes.len();
        buf[offset..(offset + len)].copy_from_slice(&last_bytes);
        offset += len;

        moosicbox_assert::assert!(
            offset == *BINARY_REQUEST_BUFFER_OFFSET,
            "Invalid binary request buffer offset {offset} != {}",
            *BINARY_REQUEST_BUFFER_OFFSET
        );

        if packet_id == 1 {
            let status_bytes = status.to_be_bytes();
            let len = status_bytes.len();
            buf[offset..(offset + len)].copy_from_slice(&status_bytes);
            offset += len;
            let headers = serde_json::to_string(&headers).unwrap();
            let headers_bytes = headers.as_bytes();
            let headers_len = u32::try_from(headers_bytes.len()).unwrap();
            let headers_len_bytes = headers_len.to_be_bytes();
            let len = headers_len_bytes.len();
            buf[offset..(offset + len)].copy_from_slice(&headers_len_bytes);
            offset += len;
            let len = headers_len as usize;
            buf[offset..(offset + len)].copy_from_slice(headers_bytes);
            offset += len;
        }

        offset
    }

    #[allow(clippy::too_many_lines)]
    async fn send_binary_stream<E: std::error::Error + Sized>(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        ranges: Option<Vec<Range>>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin + Send,
    ) -> Result<(), TunnelRequestError> {
        let mut bytes_read = 0_usize;
        let mut bytes_consumed = 0_usize;
        let mut packet_id = 1_u32;
        let mut left_over: Option<Vec<u8>> = None;
        let mut last = false;

        while !last {
            if Self::is_request_aborted(request_id, &self.abort_request_tokens) {
                log::debug!("Aborting send_binary_stream");
                break;
            }
            let mut buf = vec![0_u8; WS_MAX_PACKET_SIZE];
            let mut header_offset = Self::init_binary_request_buffer(
                request_id, packet_id, false, status, headers, &mut buf,
            );
            let mut offset = header_offset;

            let mut left_over_size = 0_usize;
            if let Some(mut left_over_str) = left_over.take() {
                if left_over_str.len() + offset > buf.len() {
                    left_over_size = buf.len() - offset;
                    left_over.replace(left_over_str.split_off(left_over_size));
                }
                let len = left_over_str.len();
                buf[offset..offset + len].copy_from_slice(&left_over_str);
                offset += len;
                bytes_consumed += len;
                left_over_size = len;
            }

            let mut packet_size = left_over_size;
            let mut packet_bytes_read = 0;

            if left_over.is_none() {
                loop {
                    match stream.next().await {
                        Some(Ok(data)) => {
                            let size = data.len();
                            bytes_read += size;
                            packet_bytes_read += size;
                            if offset + size <= WS_MAX_PACKET_SIZE {
                                buf[offset..offset + size].copy_from_slice(&data);
                                offset += size;
                                packet_size += size;
                                bytes_consumed += size;
                            } else {
                                let size_left_to_add = WS_MAX_PACKET_SIZE - offset;
                                buf[offset..WS_MAX_PACKET_SIZE]
                                    .copy_from_slice(&data[..size_left_to_add]);
                                left_over = Some(data[size_left_to_add..].to_vec());
                                offset = WS_MAX_PACKET_SIZE;
                                packet_size += size_left_to_add;
                                bytes_consumed += size_left_to_add;
                                break;
                            }
                        }
                        Some(Err(err)) => {
                            log::error!("Failed to read bytes: {err:?}");
                            return Ok(());
                        }
                        None => {
                            log::debug!("Received None");
                            buf[*BINARY_REQUEST_BUFFER_OFFSET - 1] = 1;
                            last = true;
                            break;
                        }
                    }
                }
            }

            log::debug!(
                "[{request_id}]: Read {packet_bytes_read} bytes ({bytes_read} total) last={last}"
            );

            if let Some(ranges) = &ranges {
                let mut headers_bytes = vec![0_u8; header_offset];
                let packet_start = bytes_consumed - packet_size;
                let packet_end = bytes_consumed;
                let matching_ranges = ranges
                    .iter()
                    .filter(|range| Self::does_range_overlap(range, packet_start, packet_end))
                    .collect::<Vec<_>>();

                for (i, range) in matching_ranges.iter().enumerate() {
                    if i > 0 {
                        header_offset = Self::init_binary_request_buffer(
                            request_id, packet_id, false, status, headers, &mut buf,
                        );
                    }
                    headers_bytes[0..header_offset].copy_from_slice(&buf[..header_offset]);

                    let start =
                        std::cmp::max(range.start.unwrap_or(0), packet_start) - packet_start;
                    let end =
                        std::cmp::min(range.end.unwrap_or(usize::MAX), packet_end) - packet_start;

                    if last && i == matching_ranges.len() - 1 {
                        buf[*BINARY_REQUEST_BUFFER_OFFSET - 1] = 1;
                    }

                    if let Err(err) = self.send_bytes(
                        request_id,
                        packet_id,
                        [
                            &headers_bytes[..header_offset],
                            &buf[header_offset + start..header_offset + end],
                        ]
                        .concat(),
                    ) {
                        log::error!("Failed to send bytes: {err:?}");
                        return Ok(());
                    }
                    packet_id += 1;

                    if end == bytes_consumed {
                        break;
                    }
                }
            } else {
                let bytes = &buf[..offset];
                if let Err(err) = self.send_bytes(request_id, packet_id, bytes) {
                    log::error!("Failed to send bytes: {err:?}");
                    break;
                }
                packet_id += 1;
            }
        }

        Ok(())
    }

    fn does_range_overlap(range: &Range, packet_start: usize, packet_end: usize) -> bool {
        range.start.is_none_or(|start| start < packet_end)
            && range.end.is_none_or(|end| end >= packet_start)
    }

    fn send_binary(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        mut reader: impl std::io::Read,
    ) {
        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;
        let mut last = false;

        while !last {
            if Self::is_request_aborted(request_id, &self.abort_request_tokens) {
                log::debug!("Aborting send_binary");
                break;
            }
            packet_id += 1;
            let mut buf = vec![0_u8; WS_MAX_PACKET_SIZE];
            let offset = Self::init_binary_request_buffer(
                request_id, packet_id, false, status, headers, &mut buf,
            );

            let mut read = 0;

            while offset + read < WS_MAX_PACKET_SIZE {
                match reader.read(&mut buf[offset + read..]) {
                    Ok(size) => {
                        if size == 0 {
                            buf[*BINARY_REQUEST_BUFFER_OFFSET - 1] = 1;
                            last = true;
                            break;
                        }

                        bytes_read += size;
                        read += size;
                        log::debug!("Read {size} bytes ({bytes_read} total)");
                    }
                    Err(_err) => break,
                }
            }

            let bytes = &buf[..(read + offset)];
            if let Err(err) = self.send_bytes(request_id, packet_id, bytes) {
                log::error!("Failed to send bytes: {err:?}");
                break;
            }
        }
    }

    #[cfg(feature = "base64")]
    fn init_base64_request_buffer(
        request_id: u64,
        packet_id: u32,
        status: u16,
        headers: &BTreeMap<String, String>,
        buf: &mut String,
        overflow_buf: &mut String,
    ) -> String {
        if !overflow_buf.is_empty() {
            overflow_buf.push_str(buf);
            *buf = (*overflow_buf).clone();
            "".clone_into(overflow_buf);
        }

        let mut prefix = format!("{request_id}|{packet_id}|");
        if packet_id == 1 {
            prefix.push_str(&status.to_string());
            let mut headers_base64 =
                general_purpose::STANDARD.encode(serde_json::to_string(&headers).unwrap());
            headers_base64.insert(0, '{');
            headers_base64.push('}');
            prefix.push_str(&headers_base64);
        }

        prefix
    }

    #[cfg(feature = "base64")]
    fn send_base64(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        mut reader: impl std::io::Read,
    ) -> Result<(), TunnelRequestError> {
        use std::cmp::min;

        let buf_size = 1024 * 32;
        let mut overflow_buf = String::new();

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;

        loop {
            if Self::is_request_aborted(request_id, &self.abort_request_tokens) {
                log::debug!("Aborting send_base64");
                break Ok(());
            }
            let mut buf = vec![0_u8; buf_size];
            match reader.read(&mut buf) {
                Ok(size) => {
                    packet_id += 1;
                    bytes_read += size;
                    log::debug!("Read {bytes_read} bytes");
                    let bytes = &buf[..size];
                    let mut prefix = format!("{request_id}|{packet_id}|");
                    let mut base64 = general_purpose::STANDARD.encode(bytes);

                    if packet_id == 1 {
                        prefix.push_str(&status.to_string());
                        let mut headers_base64 = general_purpose::STANDARD.encode(
                            serde_json::to_string(&headers)
                                .map_err(wrap_to_500)?
                                .clone(),
                        );
                        headers_base64.insert(0, '{');
                        headers_base64.push('}');
                        headers_base64.push_str(&base64);
                        base64 = headers_base64;
                    }

                    if !overflow_buf.is_empty() {
                        overflow_buf.push_str(&base64);
                        base64 = overflow_buf;
                        overflow_buf = String::new();
                    }
                    let end = min(base64.len(), buf_size - prefix.len());
                    let data = &base64[..end];
                    overflow_buf.push_str(&base64[end..]);
                    self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                        .map_err(wrap_to_500)?;

                    if size == 0 {
                        while !overflow_buf.is_empty() {
                            let base64 = overflow_buf;
                            overflow_buf = String::new();
                            let end = min(base64.len(), buf_size - prefix.len());
                            let data = &base64[..end];
                            overflow_buf.push_str(&base64[end..]);
                            packet_id += 1;
                            let prefix = format!("{request_id}|{packet_id}|");
                            self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                                .map_err(wrap_to_500)?;
                        }

                        packet_id += 1;
                        let prefix = format!("{request_id}|{packet_id}|");
                        self.send_message(request_id, packet_id, prefix)
                            .map_err(wrap_to_500)?;
                        break Ok(());
                    }
                }
                Err(err) => break Err(err.into()),
            }
        }
    }

    #[cfg(feature = "base64")]
    async fn send_base64_stream<E: std::error::Error + Sized>(
        &self,
        request_id: u64,
        status: u16,
        headers: &BTreeMap<String, String>,
        ranges: Option<Vec<Range>>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin + Send,
    ) -> Result<(), TunnelRequestError> {
        use std::cmp::min;

        if ranges.is_some() {
            todo!("Byte ranges for base64 not implemented");
        }

        let buf_size = 1024 * 32;
        let mut overflow_buf = String::new();

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;

        loop {
            if Self::is_request_aborted(request_id, &self.abort_request_tokens) {
                log::debug!("Aborting send_base64_stream");
                break Ok(());
            }
            packet_id += 1;

            let mut buf = String::new();

            let prefix = Self::init_base64_request_buffer(
                request_id,
                packet_id,
                status,
                headers,
                &mut buf,
                &mut overflow_buf,
            );
            let size_offset = prefix.len();

            loop {
                match stream.next().await {
                    Some(Ok(data)) => {
                        let size = data.len();
                        bytes_read += size;
                        log::debug!("Read {bytes_read} bytes");
                        let encoded = general_purpose::STANDARD.encode(data);
                        if encoded.len() + buf.len() <= buf_size - size_offset {
                            buf.push_str(&encoded);
                            if buf.len() == buf_size - size_offset {
                                break;
                            }
                        } else {
                            overflow_buf.push_str(&encoded[buf_size - size_offset - buf.len()..]);
                            buf.push_str(&encoded[..buf_size - size_offset - buf.len()]);
                            break;
                        }
                    }
                    Some(Err(err)) => {
                        log::error!("Failed to read bytes: {err:?}");
                        return Ok(());
                    }
                    None => {
                        log::debug!("Received None");
                        break;
                    }
                }
            }

            let end = min(buf.len(), buf_size - prefix.len());
            let data = &buf[..end];
            self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                .map_err(wrap_to_500)?;

            if buf.is_empty() {
                let prefix = format!("{request_id}|{packet_id}|");
                self.send_message(request_id, packet_id, prefix)
                    .map_err(wrap_to_500)?;
                break Ok(());
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn proxy_localhost_request(
        &self,
        service_port: u16,
        request_id: u64,
        method: Method,
        path: String,
        query: Value,
        payload: Option<Value>,
        headers: Option<Value>,
        profile: Option<String>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        let host = format!("http://127.0.0.1:{service_port}");
        let mut query_string = query
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                format!(
                    "{key}={}",
                    if value.is_string() {
                        value.as_str().unwrap().to_string()
                    } else {
                        value.to_string()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        if !query_string.is_empty() {
            query_string.insert(0, '?');
        }

        let url = format!("{host}/{path}{query_string}");

        self.proxy_request(
            &url, request_id, method, payload, headers, profile, encoding,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn proxy_request(
        &self,
        url: &str,
        request_id: u64,
        method: Method,
        payload: Option<Value>,
        headers: Option<Value>,
        profile: Option<String>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        let mut response = self
            .http_request(url, method, payload, headers, profile, true)
            .await?;

        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        self.send_stream(
            request_id,
            response.status().as_u16(),
            &headers,
            None,
            response.bytes_stream(),
            encoding,
        )
        .await
    }

    async fn http_request(
        &self,
        url: &str,
        method: Method,
        payload: Option<Value>,
        headers: Option<Value>,
        profile: Option<String>,
        user_agent_header: bool,
    ) -> Result<switchy_http::Response, switchy_http::Error> {
        let client = switchy_http::Client::new();

        let mut builder = client.request(method, url);

        if let Some(headers) = headers {
            for (key, value) in headers.as_object().unwrap() {
                builder = builder.header(key, value.as_str().unwrap());
            }
        }

        if let Some(profile) = profile {
            builder = builder.header("moosicbox-profile", &profile);
        }

        if user_agent_header {
            builder = builder.header("user-agent", "MOOSICBOX_TUNNEL");
        }

        if let Some(body) = payload {
            builder = builder.json(&body);
        }

        builder.send().await
    }

    /// Processes an HTTP tunnel request, forwarding it through the tunnel connection.
    ///
    /// Handles various routes including track streaming, album covers, and proxy requests.
    ///
    /// # Panics
    ///
    /// * If any of the relevant `RwLock`s are poisoned
    ///
    /// # Errors
    ///
    /// * If an error occurs processing the tunnel request
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
    pub async fn tunnel_request(
        &self,
        service_port: u16,
        request_id: u64,
        method: Method,
        path: String,
        query: Value,
        payload: Option<Value>,
        headers: Option<Value>,
        profile: Option<String>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        let abort_token = CancellationToken::new();

        {
            self.abort_request_tokens
                .write()
                .unwrap()
                .insert(request_id, abort_token.clone());
        }

        let db = profile.as_ref().and_then(|x| PROFILES.get(x));
        let music_apis = profile
            .as_ref()
            .and_then(|x| moosicbox_music_api::profiles::PROFILES.get(x));

        match path.to_lowercase().as_str() {
            "files/track" => match method {
                Method::Get => {
                    let query = serde_json::from_value::<GetTrackQuery>(query)
                        .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                    let ranges = headers
                        .and_then(|headers| {
                            headers
                                .get("range")
                                .map(|range| range.as_str().unwrap().to_string())
                        })
                        .map(|range| {
                            range.strip_prefix("bytes=").map(ToString::to_string).ok_or(
                                TunnelRequestError::BadRequest(format!(
                                    "Invalid bytes range '{range:?}'"
                                )),
                            )
                        })
                        .transpose()?
                        .map(|range| {
                            parse_ranges(&range).map_err(|e| {
                                TunnelRequestError::BadRequest(format!(
                                    "Invalid bytes range ({e:?})"
                                ))
                            })
                        })
                        .transpose()?;

                    let mut response_headers = BTreeMap::new();
                    response_headers.insert("accept-ranges".to_string(), "bytes".to_string());

                    match get_track_id_source(
                        music_apis.ok_or(TunnelRequestError::MissingProfile)?,
                        &query.track_id.into(),
                        query.source.clone().unwrap_or_else(ApiSource::library),
                        query.quality,
                    )
                    .await
                    {
                        Ok(TrackSource::LocalFilePath { path, .. }) => {
                            static CONTENT_TYPE: &str = "content-type";
                            let content_type = audio_format_to_content_type(
                                &query.format.unwrap_or(AudioFormat::Source),
                            );
                            if let Some(content_type) = content_type {
                                response_headers.insert(CONTENT_TYPE.to_string(), content_type);
                            }
                            match query.format {
                                #[cfg(feature = "format-aac")]
                                Some(AudioFormat::Aac) => {
                                    #[cfg(feature = "encoder-aac")]
                                    self.send_stream(
                                        request_id,
                                        200,
                                        &response_headers,
                                        ranges,
                                        moosicbox_audio_output::encoder::aac::encode_aac_stream(
                                            &path,
                                        ),
                                        encoding,
                                    )
                                    .await?;
                                    #[cfg(not(feature = "encoder-aac"))]
                                    panic!("No encoder-aac feature");
                                }
                                #[cfg(feature = "format-mp3")]
                                Some(AudioFormat::Mp3) => {
                                    #[cfg(feature = "encoder-mp3")]
                                    self.send_stream(
                                        request_id,
                                        200,
                                        &response_headers,
                                        ranges,
                                        moosicbox_audio_output::encoder::mp3::encode_mp3_stream(
                                            &path,
                                        ),
                                        encoding,
                                    )
                                    .await?;
                                    #[cfg(not(feature = "encoder-mp3"))]
                                    panic!("No encoder-mp3 feature");
                                }
                                #[cfg(feature = "format-opus")]
                                Some(AudioFormat::Opus) => {
                                    #[cfg(feature = "encoder-opus")]
                                    self.send_stream(
                                        request_id,
                                        200,
                                        &response_headers,
                                        ranges,
                                        moosicbox_audio_output::encoder::opus::encode_opus_stream(
                                            &path,
                                        ),
                                        encoding,
                                    )
                                    .await?;
                                    #[cfg(not(feature = "encoder-opus"))]
                                    panic!("No encoder-opus feature");
                                }
                                _ => {
                                    self.send(
                                        request_id,
                                        200,
                                        &response_headers,
                                        File::open(path)?,
                                        encoding,
                                    )?;
                                }
                            }
                        }
                        Ok(TrackSource::RemoteUrl { url, .. }) => {
                            let writer = ByteWriter::default();
                            let stream = writer.stream();

                            let get_handler = move || {
                                #[allow(unused_mut)]
                                let mut audio_output_handler = AudioDecodeHandler::new();

                                let format = match query.format {
                                    #[cfg(feature = "format-aac")]
                                    None | Some(AudioFormat::Source) => AudioFormat::Aac,
                                    #[cfg(all(
                                        not(feature = "format-aac"),
                                        feature = "format-mp3"
                                    ))]
                                    None | Some(AudioFormat::Source) => AudioFormat::Mp3,
                                    #[cfg(all(
                                        not(feature = "format-aac"),
                                        not(feature = "format-mp3"),
                                        feature = "format-opus"
                                    ))]
                                    None | Some(AudioFormat::Source) => AudioFormat::Opus,
                                    #[cfg(all(
                                        not(feature = "format-aac"),
                                        not(feature = "format-mp3"),
                                        not(feature = "format-opus")
                                    ))]
                                    None | Some(AudioFormat::Source) => {
                                        panic!("Audio format is unsupported for Tidal")
                                    }
                                    #[cfg(feature = "format-flac")]
                                    Some(AudioFormat::Flac) => {
                                        panic!("FLAC audio format is unsupported for Tidal")
                                    }
                                    #[allow(unreachable_patterns)]
                                    Some(format) => format,
                                };

                                log::debug!("Sending audio stream with format: {format:?}");

                                match format {
                                    #[cfg(feature = "format-aac")]
                                    AudioFormat::Aac => {
                                        #[cfg(feature = "encoder-aac")]
                                        {
                                            use moosicbox_audio_output::encoder::aac::AacEncoder;
                                            log::debug!("Using AAC encoder for output");
                                            audio_output_handler = audio_output_handler
                                                .with_output(Box::new(move |spec, duration| {
                                                    Ok(Box::new(
                                                        AacEncoder::with_writer(writer.clone())
                                                            .open(spec, duration),
                                                    ))
                                                }));
                                        }
                                        #[cfg(not(feature = "encoder-aac"))]
                                        panic!("No encoder-aac feature");
                                    }
                                    #[cfg(feature = "format-mp3")]
                                    AudioFormat::Mp3 => {
                                        #[cfg(feature = "encoder-mp3")]
                                        {
                                            use moosicbox_audio_output::encoder::mp3::Mp3Encoder;
                                            log::debug!("Using MP3 encoder for output");
                                            audio_output_handler = audio_output_handler
                                                .with_output(Box::new(move |spec, duration| {
                                                    Ok(Box::new(
                                                        Mp3Encoder::with_writer(writer.clone())
                                                            .open(spec, duration),
                                                    ))
                                                }));
                                        }
                                        #[cfg(not(feature = "encoder-mp3"))]
                                        panic!("No encoder-mp3 feature");
                                    }
                                    #[cfg(feature = "format-opus")]
                                    AudioFormat::Opus => {
                                        #[cfg(feature = "encoder-opus")]
                                        {
                                            use moosicbox_audio_output::encoder::opus::OpusEncoder;
                                            log::debug!("Using OPUS encoder for output");
                                            audio_output_handler = audio_output_handler
                                                .with_output(Box::new(move |spec, duration| {
                                                    Ok(Box::new(
                                                        OpusEncoder::with_writer(writer.clone())
                                                            .open(spec, duration),
                                                    ))
                                                }));
                                        }
                                        #[cfg(not(feature = "encoder-opus"))]
                                        panic!("No encoder-opus feature");
                                    }
                                    _ => {}
                                }

                                Ok(audio_output_handler)
                            };

                            log::debug!("Creating RemoteByteStream with url={url}");
                            let source: RemoteByteStreamMediaSource = RemoteByteStream::new(
                                url,
                                None,
                                true,
                                true, // HTTP range requests work for any format
                                CancellationToken::new(),
                            )
                            .into();

                            if let Err(err) = play_media_source_async(
                                MediaSourceStream::new(
                                    Box::new(source),
                                    MediaSourceStreamOptions::default(),
                                ),
                                &Hint::new(),
                                get_handler,
                                true,
                                true,
                                None,
                                None,
                            )
                            .await
                            {
                                log::error!("Failed to encode to {:?}: {err:?}", query.format);
                            }

                            self.send_stream(
                                request_id,
                                200,
                                &response_headers,
                                ranges,
                                stream,
                                encoding,
                            )
                            .await?;
                        }
                        Err(err) => {
                            log::error!("Failed to get track source: {err:?}");
                        }
                    }

                    Ok(())
                }
                Method::Head => {
                    self.proxy_localhost_request(
                        service_port,
                        request_id,
                        method,
                        path,
                        query,
                        payload,
                        headers,
                        profile,
                        encoding,
                    )
                    .await
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            "files/track/info" => match method {
                Method::Get => {
                    let query = serde_json::from_value::<GetTrackInfoQuery>(query)
                        .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                    let mut headers = BTreeMap::new();
                    headers.insert("content-type".to_string(), "application/json".to_string());

                    let music_apis = music_apis.ok_or(TunnelRequestError::MissingProfile)?;
                    let api = music_apis
                        .get(&query.source.clone().unwrap_or_else(ApiSource::library))
                        .ok_or_else(|| {
                            TunnelRequestError::BadRequest("Invalid source".to_string())
                        })?;

                    if let Ok(track_info) = get_track_info(&**api, &query.track_id.into()).await {
                        let mut bytes: Vec<u8> = Vec::new();
                        serde_json::to_writer(&mut bytes, &track_info)?;
                        self.send(request_id, 200, &headers, Cursor::new(bytes), encoding)?;
                    }

                    Ok(())
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            _ => {
                let re = Regex::new(r"^files/albums/(\d+)/(\d+)x(\d+)$")?;
                if let Some(caps) = re.captures(&path) {
                    match method {
                        Method::Get => {
                            let query = serde_json::from_value::<AlbumCoverQuery>(query)
                                .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                            let album_id_string = caps
                                .get(1)
                                .ok_or(TunnelRequestError::BadRequest("Invalid album_id".into()))?
                                .as_str();

                            let source = query.source.clone().unwrap_or_else(ApiSource::library);
                            let album_id = if source.is_library() {
                                album_id_string.parse::<u64>().map(Id::Number)
                            } else {
                                Ok(Id::String(album_id_string.to_owned()))
                            }
                            .map_err(|_| {
                                TunnelRequestError::BadRequest("Invalid album_id".into())
                            })?;

                            let width = caps
                                .get(2)
                                .ok_or(TunnelRequestError::BadRequest("Missing width".into()))?
                                .as_str()
                                .parse::<u32>()
                                .map_err(|_| TunnelRequestError::BadRequest("Bad width".into()))?;
                            let height = caps
                                .get(3)
                                .ok_or(TunnelRequestError::BadRequest("Missing height".into()))?
                                .as_str()
                                .parse::<u32>()
                                .map_err(|_| TunnelRequestError::BadRequest("Bad height".into()))?;

                            let music_apis =
                                music_apis.ok_or(TunnelRequestError::MissingProfile)?;
                            let api = music_apis
                                .get(&query.source.clone().unwrap_or_else(ApiSource::library))
                                .ok_or_else(|| {
                                    TunnelRequestError::BadRequest("Invalid source".to_string())
                                })?;

                            let album = api
                                .album(&album_id)
                                .await
                                .map_err(|e| {
                                    TunnelRequestError::NotFound(format!(
                                        "Failed to get album: {e:?}"
                                    ))
                                })?
                                .ok_or_else(|| {
                                    TunnelRequestError::NotFound(format!(
                                        "Album not found: {}",
                                        album_id.clone()
                                    ))
                                })?;

                            let path = get_album_cover(
                                &**api,
                                &db.ok_or(TunnelRequestError::MissingProfile)?,
                                &album,
                                u16::try_from(std::cmp::max(width, height)).unwrap().into(),
                            )
                            .await
                            .map_err(|e| TunnelRequestError::Request(e.to_string()))?;

                            let mut headers = BTreeMap::new();
                            let resized = {
                                use moosicbox_image::{
                                    Encoding, image::try_resize_local_file_async,
                                };
                                if let Some(resized) = try_resize_local_file_async(
                                    width,
                                    height,
                                    &path,
                                    Encoding::Webp,
                                    80,
                                )
                                .await
                                .map_err(|e| {
                                    TunnelRequestError::InternalServerError(Box::new(
                                        AlbumCoverError::File(path.clone(), e.to_string()),
                                    ))
                                })? {
                                    headers.insert(
                                        "content-type".to_string(),
                                        "image/webp".to_string(),
                                    );
                                    resized
                                } else {
                                    headers.insert(
                                        "content-type".to_string(),
                                        "image/jpeg".to_string(),
                                    );
                                    try_resize_local_file_async(
                                        width,
                                        height,
                                        &path,
                                        Encoding::Jpeg,
                                        80,
                                    )
                                    .await
                                    .map_err(|e| {
                                        TunnelRequestError::InternalServerError(Box::new(
                                            AlbumCoverError::File(path.clone(), e.to_string()),
                                        ))
                                    })?
                                    .ok_or_else(|| {
                                        TunnelRequestError::InternalServerError(Box::new(
                                            AlbumCoverError::File(
                                                path,
                                                "No cover from Option".to_string(),
                                            ),
                                        ))
                                    })?
                                }
                            };

                            headers.insert(
                                "cache-control".to_string(),
                                format!("max-age={}", 86400u32 * 14),
                            );
                            self.send(request_id, 200, &headers, Cursor::new(resized), encoding)?;

                            Ok(())
                        }
                        _ => Err(TunnelRequestError::UnsupportedMethod),
                    }
                } else {
                    self.proxy_localhost_request(
                        service_port,
                        request_id,
                        method,
                        path,
                        query,
                        payload,
                        headers,
                        profile,
                        encoding,
                    )
                    .await?;

                    Ok(())
                }
            }
        }
    }

    /// Processes a websocket tunnel request, handling player actions and message forwarding.
    ///
    /// # Panics
    ///
    /// * If any of the relevant `RwLock`s are poisoned
    ///
    /// # Errors
    ///
    /// * If the websocket request fails to process
    pub async fn ws_request(
        &self,
        conn_id: u64,
        request_id: u64,
        value: Value,
        profile: Option<String>,
        sender: impl WebsocketSender,
    ) -> Result<(), TunnelRequestError> {
        let context = WebsocketContext {
            connection_id: conn_id.to_string(),
            profile: profile.clone(),
            player_actions: self.player_actions.read().unwrap().clone(),
        };
        let packet_id = 1_u32;
        log::debug!(
            "Processing tunnel ws request request_id={request_id} packet_id={packet_id} conn_id={conn_id}"
        );
        let sender = TunnelWebsocketSender {
            id: self.id,
            propagate_id: conn_id,
            packet_id,
            request_id,
            root_sender: sender,
            tunnel_sender: self.sender.read().unwrap().clone().unwrap(),
            profile,
        };
        moosicbox_ws::process_message(&self.config_db, value, context, &sender).await?;
        log::debug!("Processed tunnel ws request {request_id} {packet_id}");
        Ok(())
    }

    /// Aborts an in-progress tunnel request by cancelling its token.
    ///
    /// # Panics
    ///
    /// * If the `abort_request_tokens` `RwLock` is poisoned
    pub fn abort_request(&self, request_id: u64) {
        if let Some(token) = self.abort_request_tokens.read().unwrap().get(&request_id) {
            token.cancel();
        }
    }
}
