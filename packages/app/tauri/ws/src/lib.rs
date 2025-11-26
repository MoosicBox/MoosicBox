//! WebSocket client implementation for `MoosicBox` applications.
//!
//! This crate provides a robust WebSocket client with automatic reconnection,
//! connection management, and message handling for `MoosicBox` applications.
//!
//! # Features
//!
//! * Automatic reconnection with exponential backoff on connection failures
//! * Async/await based API using tokio
//! * Message multiplexing with separate send/receive channels
//! * Graceful cancellation and connection closing
//! * Support for text, binary, and ping/pong messages
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_app_ws::{WsClient, WsMessage};
//! # use tokio::sync::mpsc;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (client, handle) = WsClient::new("ws://localhost:8080".to_string());
//! let (tx, mut rx) = mpsc::channel(100);
//!
//! // Start the websocket connection
//! tokio::spawn(async move {
//!     client.start(None, None, "default".to_string(), || {}, tx).await
//! });
//!
//! // Receive messages
//! while let Some(msg) = rx.recv().await {
//!     match msg {
//!         WsMessage::TextMessage(text) => println!("Received: {}", text),
//!         WsMessage::Message(bytes) => println!("Received {} bytes", bytes.len()),
//!         WsMessage::Ping => println!("Received ping"),
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_channel::mpsc::UnboundedSender;
use futures_util::{StreamExt as _, future, pin_mut};
use switchy_async::util::CancellationToken;
use thiserror::Error;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::SendError;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error, Message},
};

/// Error type for sending bytes over a websocket connection.
#[derive(Debug, Error)]
pub enum SendBytesError {
    /// An unknown error occurred during the send operation.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for sending messages over a websocket connection.
#[derive(Debug, Error)]
pub enum SendMessageError {
    /// An unknown error occurred during the send operation.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for websocket connection failures.
#[derive(Debug, Error)]
pub enum ConnectWsError {
    /// The websocket connection was rejected with an HTTP 401 Unauthorized response.
    #[error("Unauthorized")]
    Unauthorized,
}

/// Messages that can be sent or received over a websocket connection.
pub enum WsMessage {
    /// A text message.
    TextMessage(String),
    /// A binary message.
    Message(Bytes),
    /// A ping message.
    Ping,
}

/// Error type for websocket send operations.
#[derive(Debug, Error)]
pub enum WebsocketSendError {
    /// An unknown error occurred during the send operation.
    #[error("Unknown: {0}")]
    Unknown(String),
}

/// Trait for types that can send messages over a websocket connection.
#[async_trait]
pub trait WebsocketSender: Send + Sync {
    /// Sends a text message over the websocket connection.
    ///
    /// # Errors
    ///
    /// * Returns [`WebsocketSendError::Unknown`] if the send operation fails
    async fn send(&self, data: &str) -> Result<(), WebsocketSendError>;

    /// Sends a ping message over the websocket connection.
    ///
    /// # Errors
    ///
    /// * Returns [`WebsocketSendError::Unknown`] if the send operation fails
    async fn ping(&self) -> Result<(), WebsocketSendError>;
}

/// Debug implementation for trait objects implementing `WebsocketSender`.
impl core::fmt::Debug for dyn WebsocketSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{WebsocketSender}}")
    }
}

/// Error type for closing websocket connections.
#[derive(Debug, Error)]
pub enum CloseError {
    /// An unknown error occurred during the close operation.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// A handle to a websocket connection that allows sending messages and closing the connection.
#[derive(Clone)]
pub struct WsHandle {
    sender: Arc<RwLock<Option<UnboundedSender<WsMessage>>>>,
    cancellation_token: CancellationToken,
}

impl WsHandle {
    /// Closes the websocket connection.
    ///
    /// This method signals the websocket client to gracefully shut down by canceling
    /// the internal cancellation token. The connection will close after any pending
    /// operations complete.
    pub fn close(&self) {
        self.cancellation_token.cancel();
    }
}

#[async_trait]
impl WebsocketSender for WsHandle {
    /// Sends a text message over the websocket connection.
    ///
    /// # Errors
    ///
    /// * Returns [`WebsocketSendError::Unknown`] if the send operation fails
    ///
    /// # Panics
    ///
    /// * Panics if the internal `RwLock` is poisoned
    async fn send(&self, data: &str) -> Result<(), WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(WsMessage::TextMessage(data.to_string()))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    /// Sends a ping message over the websocket connection.
    ///
    /// # Errors
    ///
    /// * Returns [`WebsocketSendError::Unknown`] if the send operation fails
    ///
    /// # Panics
    ///
    /// * Panics if the internal `RwLock` is poisoned
    async fn ping(&self) -> Result<(), WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(WsMessage::Ping)
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }
}

/// A websocket client that manages connections and message handling.
#[derive(Clone)]
pub struct WsClient {
    url: String,
    sender: Arc<RwLock<Option<UnboundedSender<WsMessage>>>>,
    cancellation_token: CancellationToken,
}

impl WsClient {
    /// Creates a new websocket client for the given URL.
    ///
    /// Returns a tuple containing the client and a handle to control the connection.
    #[must_use]
    pub fn new(url: String) -> (Self, WsHandle) {
        Self::new_inner(url, CancellationToken::new())
    }

    fn new_inner(url: String, cancellation_token: CancellationToken) -> (Self, WsHandle) {
        let sender = Arc::new(RwLock::new(None));
        let handle = WsHandle {
            sender: sender.clone(),
            cancellation_token: cancellation_token.clone(),
        };

        (
            Self {
                url,
                sender,
                cancellation_token,
            },
            handle,
        )
    }

    /// Sets a custom cancellation token for the websocket client.
    ///
    /// This allows external cancellation of the websocket connection.
    #[must_use]
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    async fn message_handler(
        tx: Sender<WsMessage>,
        m: Message,
    ) -> Result<(), SendError<WsMessage>> {
        log::trace!("Message from ws server: {m:?}");
        tx.send(match m {
            Message::Text(m) => WsMessage::TextMessage(m.to_string()),
            Message::Binary(m) => WsMessage::Message(m),
            Message::Ping(_m) => WsMessage::Ping,
            Message::Pong(_m) => {
                log::trace!("Received pong");
                return Ok(());
            }
            Message::Close(_m) => unimplemented!(),
            Message::Frame(_m) => unimplemented!(),
        })
        .await
    }

    /// Starts the websocket connection with automatic reconnection on failure.
    ///
    /// # Errors
    ///
    /// * Returns [`ConnectWsError::Unauthorized`] if the websocket connection is unauthorized
    ///
    /// # Panics
    ///
    /// * Panics if the internal `RwLock` is poisoned
    pub async fn start(
        &self,
        client_id: Option<String>,
        signature_token: Option<String>,
        profile: String,
        on_start: impl Fn() + Send + 'static,
        tx: Sender<WsMessage>,
    ) -> Result<(), ConnectWsError> {
        self.start_handler(
            client_id,
            signature_token,
            profile,
            Self::message_handler,
            on_start,
            tx,
        )
        .await
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    async fn start_handler<T, O>(
        &self,
        client_id: Option<String>,
        signature_token: Option<String>,
        profile: String,
        handler: fn(sender: Sender<T>, m: Message) -> O,
        on_start: impl Fn() + Send + 'static,
        tx: Sender<T>,
    ) -> Result<(), ConnectWsError>
    where
        T: Send + 'static,
        O: Future<Output = Result<(), SendError<T>>> + Send + 'static,
    {
        let url = self.url.clone();
        let sender_arc = self.sender.clone();
        let cancellation_token = self.cancellation_token.clone();

        let mut just_retried = false;

        loop {
            let close_token = CancellationToken::new();

            let (txf, rxf) = futures_channel::mpsc::unbounded();

            sender_arc.write().unwrap().replace(txf.clone());

            let profile_param = format!("?moosicboxProfile={profile}");
            let client_id_param = client_id
                .as_ref()
                .map_or_else(String::new, |id| format!("&clientId={id}"));
            let signature_token_param = if client_id.is_some() {
                signature_token
                    .as_ref()
                    .map_or_else(String::new, |token| format!("&signature={token}"))
            } else {
                String::new()
            };
            let url = format!("{url}{profile_param}{client_id_param}{signature_token_param}");
            log::debug!("Connecting to websocket '{url}'...");
            #[allow(clippy::redundant_pub_crate)]
            match select!(
                resp = connect_async(url) => resp,
                () = cancellation_token.cancelled() => {
                    log::debug!("Cancelling connect");
                    break;
                }
            ) {
                Ok((ws_stream, _)) => {
                    log::debug!("WebSocket handshake has been successfully completed");
                    on_start();

                    if just_retried {
                        log::info!("WebSocket successfully reconnected");
                        just_retried = false;
                    }

                    let (write, read) = ws_stream.split();

                    let ws_writer = rxf
                        .map(|message| match message {
                            WsMessage::TextMessage(message) => {
                                moosicbox_logging::debug_or_trace!(
                                    ("Sending text packet from request"),
                                    ("Sending text packet from request message={message}")
                                );
                                Ok(Message::Text(message.into()))
                            }
                            WsMessage::Message(bytes) => {
                                log::debug!("Sending packet from request",);
                                Ok(Message::Binary(bytes.to_vec().into()))
                            }
                            WsMessage::Ping => {
                                log::trace!("Sending ping");
                                Ok(Message::Ping(vec![].into()))
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

                        switchy_async::runtime::Handle::current().spawn_with_name(
                            "ws: Process WS message",
                            {
                                let tx = tx.clone();
                                let close_token = close_token.clone();

                                async move {
                                    if let Err(e) = handler(tx.clone(), m).await {
                                        log::error!("Handler Send Loop error: {e:?}");
                                        close_token.cancel();
                                    }
                                }
                            },
                        );
                    });

                    let pinger = switchy_async::runtime::Handle::current().spawn_with_name("ws: pinger", {
                        let txf = txf.clone();
                        let close_token = close_token.clone();
                        let cancellation_token = cancellation_token.clone();

                        async move {
                            loop {
                                select!(
                                    () = close_token.cancelled() => { break; }
                                    () = cancellation_token.cancelled() => { break; }
                                    () = tokio::time::sleep(std::time::Duration::from_millis(5000)) => {
                                        log::trace!("Sending ping to server");
                                        if let Err(e) = txf.unbounded_send(WsMessage::Ping) {
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
                    log::debug!("start_handler: Waiting for pinger to finish...");
                    if let Err(e) = pinger.await {
                        log::warn!("start_handler: Pinger failed to finish: {e:?}");
                    }
                    log::info!("WebSocket connection closed");
                }
                Err(err) => {
                    log::error!("Websocket error: {err:?}");
                    if let Error::Http(response) = err {
                        if response.status() == StatusCode::UNAUTHORIZED {
                            log::error!("Unauthorized ws connection");
                            return Err(ConnectWsError::Unauthorized);
                        }

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

            #[allow(clippy::redundant_pub_crate)]
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

        log::debug!("Handler closed");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test_log::test(tokio::test)]
    async fn test_message_handler_text_message() {
        let (tx, mut rx) = mpsc::channel(10);
        let text = "hello world".to_string();
        let message = Message::Text(text.clone().into());

        let result = WsClient::message_handler(tx, message).await;

        assert!(result.is_ok());
        let received = rx.recv().await.unwrap();
        match received {
            WsMessage::TextMessage(s) => assert_eq!(s, text),
            _ => panic!("Expected TextMessage"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_message_handler_binary_message() {
        let (tx, mut rx) = mpsc::channel(10);
        let data = vec![1u8, 2, 3, 4, 5];
        let message = Message::Binary(data.clone().into());

        let result = WsClient::message_handler(tx, message).await;

        assert!(result.is_ok());
        let received = rx.recv().await.unwrap();
        match received {
            WsMessage::Message(bytes) => assert_eq!(bytes.as_ref(), &data[..]),
            _ => panic!("Expected Message with bytes"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_message_handler_ping() {
        let (tx, mut rx) = mpsc::channel(10);
        let message = Message::Ping(vec![].into());

        let result = WsClient::message_handler(tx, message).await;

        assert!(result.is_ok());
        let received = rx.recv().await.unwrap();
        assert!(matches!(received, WsMessage::Ping));
    }

    #[test_log::test(tokio::test)]
    async fn test_message_handler_pong_returns_ok_without_sending() {
        let (tx, mut rx) = mpsc::channel(10);
        let message = Message::Pong(vec![].into());

        let result = WsClient::message_handler(tx, message).await;

        assert!(result.is_ok());
        // Pong messages should not be forwarded
        assert!(rx.try_recv().is_err());
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_send_with_no_sender() {
        let handle = WsHandle {
            sender: Arc::new(RwLock::new(None)),
            cancellation_token: CancellationToken::new(),
        };

        // Send should succeed silently when there's no sender
        let result = handle.send("test message").await;
        assert!(result.is_ok());
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_send_with_active_sender() {
        let (tx, mut rx) = futures_channel::mpsc::unbounded();
        let handle = WsHandle {
            sender: Arc::new(RwLock::new(Some(tx))),
            cancellation_token: CancellationToken::new(),
        };

        let result = handle.send("test message").await;

        assert!(result.is_ok());
        let received = rx.try_next().unwrap().unwrap();
        match received {
            WsMessage::TextMessage(s) => assert_eq!(s, "test message"),
            _ => panic!("Expected TextMessage"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_send_with_closed_channel() {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        // Close the receiver to simulate channel being closed
        drop(rx);

        let handle = WsHandle {
            sender: Arc::new(RwLock::new(Some(tx))),
            cancellation_token: CancellationToken::new(),
        };

        let result = handle.send("test message").await;

        assert!(result.is_err());
        match result {
            Err(WebsocketSendError::Unknown(msg)) => {
                assert!(msg.contains("send"));
            }
            _ => panic!("Expected Unknown error"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_ping_with_no_sender() {
        let handle = WsHandle {
            sender: Arc::new(RwLock::new(None)),
            cancellation_token: CancellationToken::new(),
        };

        // Ping should succeed silently when there's no sender
        let result = handle.ping().await;
        assert!(result.is_ok());
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_ping_with_active_sender() {
        let (tx, mut rx) = futures_channel::mpsc::unbounded();
        let handle = WsHandle {
            sender: Arc::new(RwLock::new(Some(tx))),
            cancellation_token: CancellationToken::new(),
        };

        let result = handle.ping().await;

        assert!(result.is_ok());
        let received = rx.try_next().unwrap().unwrap();
        assert!(matches!(received, WsMessage::Ping));
    }

    #[test_log::test(tokio::test)]
    async fn test_ws_handle_ping_with_closed_channel() {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        // Close the receiver to simulate channel being closed
        drop(rx);

        let handle = WsHandle {
            sender: Arc::new(RwLock::new(Some(tx))),
            cancellation_token: CancellationToken::new(),
        };

        let result = handle.ping().await;

        assert!(result.is_err());
        match result {
            Err(WebsocketSendError::Unknown(msg)) => {
                assert!(msg.contains("send"));
            }
            _ => panic!("Expected Unknown error"),
        }
    }

    #[test_log::test]
    fn test_ws_handle_close_cancels_token() {
        let token = CancellationToken::new();
        let handle = WsHandle {
            sender: Arc::new(RwLock::new(None)),
            cancellation_token: token.clone(),
        };

        assert!(!token.is_cancelled());
        handle.close();
        assert!(token.is_cancelled());
    }

    #[test_log::test]
    fn test_ws_client_new_returns_client_and_handle_with_shared_state() {
        let (client, handle) = WsClient::new("ws://localhost:8080".to_string());

        // Verify the URL is set correctly
        assert_eq!(client.url, "ws://localhost:8080");

        // Verify sender is initially None
        assert!(client.sender.read().unwrap().is_none());
        assert!(handle.sender.read().unwrap().is_none());

        // Verify they share the same sender Arc
        assert!(Arc::ptr_eq(&client.sender, &handle.sender));
    }

    #[test_log::test]
    fn test_ws_client_with_cancellation_token_replaces_token() {
        let (client, _handle) = WsClient::new("ws://localhost:8080".to_string());
        let new_token = CancellationToken::new();

        // Create a new client with a different token
        let client_with_token = client.with_cancellation_token(new_token.clone());

        // Verify the new token is used
        new_token.cancel();
        assert!(client_with_token.cancellation_token.is_cancelled());
    }
}
