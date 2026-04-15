use std::{collections::BTreeMap, sync::RwLock};

use async_trait::async_trait;
use flume::{Receiver, Sender};
use futures_util::StreamExt as _;
use hyperchad_shared_state_models::{TransportInbound, TransportOutbound};
use reqwest::{Client, header::HeaderMap};
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{SharedStateTransportClient, TransportError};

#[derive(Debug)]
pub struct SsePostJsonTransportClient {
    sse_url: String,
    post_url: String,
    headers: BTreeMap<String, String>,
    client: Client,
    inbound_tx: Sender<TransportInbound>,
    inbound_rx: Receiver<TransportInbound>,
    close_tx: RwLock<Option<oneshot::Sender<()>>>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl SsePostJsonTransportClient {
    /// # Errors
    ///
    /// * [`TransportError::Operation`] - If HTTP client construction fails
    pub fn new(
        sse_url: impl Into<String>,
        post_url: impl Into<String>,
    ) -> Result<Self, TransportError> {
        Self::new_with_headers(sse_url, post_url, BTreeMap::new())
    }

    /// # Errors
    ///
    /// * [`TransportError::Operation`] - If HTTP client construction fails
    pub fn new_with_headers(
        sse_url: impl Into<String>,
        post_url: impl Into<String>,
        headers: BTreeMap<String, String>,
    ) -> Result<Self, TransportError> {
        let (inbound_tx, inbound_rx) = flume::unbounded();
        let client = Client::builder()
            .build()
            .map_err(|e| TransportError::Operation(format!("Failed to build HTTP client: {e}")))?;

        Ok(Self {
            sse_url: sse_url.into(),
            post_url: post_url.into(),
            headers,
            client,
            inbound_tx,
            inbound_rx,
            close_tx: RwLock::new(None),
            task: std::sync::Mutex::new(None),
        })
    }

    fn lock_poison_error(context: &str) -> TransportError {
        TransportError::Operation(format!("{context}: lock poisoned"))
    }

    fn request_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap, TransportError> {
        let mut header_map = HeaderMap::new();

        for (key, value) in headers {
            let header_name =
                reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                    TransportError::Operation(format!("Invalid header name '{key}': {e}"))
                })?;
            let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|e| {
                TransportError::Operation(format!("Invalid header value for '{key}': {e}"))
            })?;
            header_map.insert(header_name, header_value);
        }

        Ok(header_map)
    }

    fn parse_sse_frame(frame: &str) -> Option<String> {
        let mut data_lines = Vec::new();

        for line in frame.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                data_lines.push(data.trim_start());
            }
        }

        if data_lines.is_empty() {
            None
        } else {
            Some(data_lines.join("\n"))
        }
    }
}

#[async_trait]
impl SharedStateTransportClient for SsePostJsonTransportClient {
    async fn connect(&self) -> Result<(), TransportError> {
        if self
            .task
            .lock()
            .map_err(|_| Self::lock_poison_error("connect task"))?
            .is_some()
        {
            return Ok(());
        }

        let headers = Self::request_headers(&self.headers)?;
        let response = self
            .client
            .get(&self.sse_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| TransportError::Operation(format!("SSE connection failed: {e}")))?;

        if !response.status().is_success() {
            return Err(TransportError::Operation(format!(
                "SSE connection failed with status {}",
                response.status()
            )));
        }

        let (close_tx, mut close_rx) = oneshot::channel();
        *self
            .close_tx
            .write()
            .map_err(|_| Self::lock_poison_error("set close sender"))? = Some(close_tx);

        let inbound_tx = self.inbound_tx.clone();
        let mut stream = response.bytes_stream();

        let task = tokio::spawn(async move {
            let mut buffer = String::new();

            loop {
                tokio::select! {
                    _ = &mut close_rx => {
                        break;
                    }
                    chunk = stream.next() => {
                        let Some(chunk) = chunk else {
                            break;
                        };

                        match chunk {
                            Ok(chunk) => {
                                let chunk_str = String::from_utf8_lossy(&chunk);
                                buffer.push_str(&chunk_str);

                                while let Some(separator_index) = buffer.find("\n\n") {
                                    let mut frame = buffer[..separator_index].to_string();
                                    buffer.drain(..separator_index + 2);
                                    frame = frame.replace("\r\n", "\n");

                                    if let Some(data) = Self::parse_sse_frame(&frame) {
                                        match serde_json::from_str::<TransportInbound>(&data) {
                                            Ok(message) => {
                                                if inbound_tx.send(message).is_err() {
                                                    return;
                                                }
                                            }
                                            Err(error) => {
                                                log::warn!("Failed to decode SSE frame as transport message: {error}");
                                            }
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                log::warn!("SSE stream read failed: {error}");
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
            .map_err(|_| Self::lock_poison_error("store task"))? = Some(task);

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
                "SSE task join failed: {error}"
            )));
        }

        Ok(())
    }

    async fn send(&self, message: TransportOutbound) -> Result<(), TransportError> {
        let payload = serde_json::to_vec(&message).map_err(|e| {
            TransportError::Operation(format!("Failed to serialize transport outbound: {e}"))
        })?;

        let headers = Self::request_headers(&self.headers)?;
        let response = self
            .client
            .post(&self.post_url)
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|e| TransportError::Operation(format!("POST transport send failed: {e}")))?;

        if !response.status().is_success() {
            return Err(TransportError::Operation(format!(
                "POST transport send failed with status {}",
                response.status()
            )));
        }

        Ok(())
    }

    fn inbound(&self) -> Receiver<TransportInbound> {
        self.inbound_rx.clone()
    }
}
