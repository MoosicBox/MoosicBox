use std::{collections::BTreeMap, sync::RwLock};

use async_trait::async_trait;
use flume::{Receiver, Sender};
use futures_util::{SinkExt as _, StreamExt as _};
use hyperchad_shared_state_models::{TransportInbound, TransportOutbound};
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_tungstenite::tungstenite::Message;

use crate::{SharedStateTransportClient, TransportError};

#[derive(Debug)]
pub struct WebSocketJsonTransportClient {
    url: String,
    headers: BTreeMap<String, String>,
    inbound_tx: Sender<TransportInbound>,
    inbound_rx: Receiver<TransportInbound>,
    outbound_tx: RwLock<Option<Sender<TransportOutbound>>>,
    close_tx: RwLock<Option<oneshot::Sender<()>>>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl WebSocketJsonTransportClient {
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self::new_with_headers(url, BTreeMap::new())
    }

    #[must_use]
    pub fn new_with_headers(url: impl Into<String>, headers: BTreeMap<String, String>) -> Self {
        let (inbound_tx, inbound_rx) = flume::unbounded();

        Self {
            url: url.into(),
            headers,
            inbound_tx,
            inbound_rx,
            outbound_tx: RwLock::new(None),
            close_tx: RwLock::new(None),
            task: std::sync::Mutex::new(None),
        }
    }

    fn lock_poison_error(context: &str) -> TransportError {
        TransportError::Operation(format!("{context}: lock poisoned"))
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl SharedStateTransportClient for WebSocketJsonTransportClient {
    async fn connect(&self) -> Result<(), TransportError> {
        if self
            .task
            .lock()
            .map_err(|_| Self::lock_poison_error("connect task"))?
            .is_some()
        {
            return Ok(());
        }

        let mut request = http::Request::builder().method("GET").uri(&self.url);
        for (header, value) in &self.headers {
            request = request.header(header, value);
        }

        let request = request
            .body(())
            .map_err(|e| TransportError::Operation(format!("Failed to build request: {e}")))?;

        let (stream, _response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| TransportError::Operation(format!("WebSocket connection failed: {e}")))?;

        let (mut writer, mut reader) = stream.split();
        let inbound_tx = self.inbound_tx.clone();
        let (outbound_tx, outbound_rx) = flume::unbounded::<TransportOutbound>();
        let (close_tx, mut close_rx) = oneshot::channel::<()>();

        *self
            .outbound_tx
            .write()
            .map_err(|_| Self::lock_poison_error("set outbound sender"))? = Some(outbound_tx);
        *self
            .close_tx
            .write()
            .map_err(|_| Self::lock_poison_error("set close sender"))? = Some(close_tx);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut close_rx => {
                        if let Err(error) = writer.close().await {
                            log::debug!("WebSocket close failed: {error}");
                        }
                        break;
                    }
                    outbound = outbound_rx.recv_async() => {
                        match outbound {
                            Ok(outbound) => {
                                match serde_json::to_string(&outbound) {
                                    Ok(payload) => {
                                        if let Err(error) = writer.send(Message::Text(payload.into())).await {
                                            log::warn!("WebSocket send failed: {error}");
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        log::warn!("Failed to serialize outbound transport message: {error}");
                                    }
                                }
                            }
                            Err(_error) => {
                                break;
                            }
                        }
                    }
                    inbound = reader.next() => {
                        let Some(inbound) = inbound else {
                            break;
                        };

                        match inbound {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<TransportInbound>(&text) {
                                    Ok(message) => {
                                        if inbound_tx.send(message).is_err() {
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        log::warn!("Failed to decode inbound websocket message: {error}");
                                    }
                                }
                            }
                            Ok(Message::Binary(data)) => {
                                match serde_json::from_slice::<TransportInbound>(&data) {
                                    Ok(message) => {
                                        if inbound_tx.send(message).is_err() {
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        log::warn!("Failed to decode binary websocket message: {error}");
                                    }
                                }
                            }
                            Ok(Message::Ping(payload)) => {
                                if let Err(error) = writer.send(Message::Pong(payload)).await {
                                    log::debug!("Failed to respond with pong: {error}");
                                    break;
                                }
                            }
                            Ok(Message::Close(_frame)) => {
                                break;
                            }
                            Ok(Message::Pong(_) | Message::Frame(_)) => {}
                            Err(error) => {
                                log::warn!("WebSocket receive failed: {error}");
                                break;
                            }
                        }
                    }
                }
            }
        });

        *self
            .task
            .lock()
            .map_err(|_| Self::lock_poison_error("store task handle"))? = Some(task);

        Ok(())
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        let close_tx = self
            .close_tx
            .write()
            .map_err(|_| Self::lock_poison_error("disconnect close channel"))?
            .take();

        if let Some(close_tx) = close_tx {
            let _ = close_tx.send(());
        }

        let task = {
            self.task
                .lock()
                .map_err(|_| Self::lock_poison_error("disconnect task"))?
                .take()
        };

        if let Some(task) = task
            && let Err(error) = task.await
        {
            return Err(TransportError::Operation(format!(
                "WebSocket task join failed: {error}"
            )));
        }

        *self
            .outbound_tx
            .write()
            .map_err(|_| Self::lock_poison_error("clear outbound sender"))? = None;

        Ok(())
    }

    async fn send(&self, message: TransportOutbound) -> Result<(), TransportError> {
        let sender = self
            .outbound_tx
            .read()
            .map_err(|_| Self::lock_poison_error("send outbound read"))?
            .as_ref()
            .cloned()
            .ok_or(TransportError::Disconnected)?;

        sender
            .send(message)
            .map_err(|_| TransportError::Disconnected)
    }

    fn inbound(&self) -> Receiver<TransportInbound> {
        self.inbound_rx.clone()
    }
}
