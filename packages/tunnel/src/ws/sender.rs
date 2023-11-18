use std::{collections::HashMap, sync::Mutex, thread, time::Duration};

use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, SendError};
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::runtime::{self, Runtime};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{protocol::frame::Frame, Error, Message},
};

use crate::ws::WS_HOST;

use super::SENDER;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub enum TunnelMessage {
    Text(String),
    Binary(Bytes),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
    Frame(Frame),
}

fn server_handler(
    tx: crossbeam_channel::Sender<TunnelMessage>,
    m: Message,
) -> Result<(), SendError<TunnelMessage>> {
    trace!("Message from tunnel ws server: {m:?}");
    tx.send(match m {
        Message::Text(m) => TunnelMessage::Text(m),
        Message::Binary(m) => TunnelMessage::Binary(Bytes::from(m)),
        Message::Ping(m) => TunnelMessage::Ping(m),
        Message::Pong(m) => TunnelMessage::Pong(m),
        Message::Close(_m) => TunnelMessage::Close,
        Message::Frame(m) => TunnelMessage::Frame(m),
    })
}

pub fn start_server(client_id: String) -> Receiver<TunnelMessage> {
    start(client_id, server_handler)
}

pub struct TunnelResponse {
    pub request_id: usize,
    pub packet_id: u32,
    pub bytes: Bytes,
}

fn serverless_handler(
    tx: crossbeam_channel::Sender<TunnelResponse>,
    m: Message,
) -> Result<(), SendError<TunnelResponse>> {
    static REQUEST_BUFFERS: Lazy<Mutex<HashMap<usize, String>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    static REQUEST_PACKET_IDS: Lazy<Mutex<HashMap<usize, u32>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    static REQUEST_QUEUES: Lazy<Mutex<HashMap<usize, Vec<(u32, String)>>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    trace!("Message from tunnel ws lambda server");
    let message = match m {
        Message::Text(mut data) => {
            let content_start = data
                .chars()
                .position(|c| c == '|')
                .expect("Invalid content start. Expected '|' delimiter");
            let temp = data.split_off(content_start);
            let request_id = data.parse::<usize>().unwrap();
            let mut data = temp;
            data.remove(0); // Remove '|'

            let content_start = data
                .chars()
                .position(|c| c == '|')
                .expect("Invalid content start. Expected '|' delimiter");
            let temp = data.split_off(content_start);
            let packet_id = data.parse::<u32>().unwrap();
            let mut data = temp;
            data.remove(0); // Remove '|'

            let current_packet_id = REQUEST_PACKET_IDS
                .lock()
                .unwrap()
                .get(&request_id)
                .copied()
                .unwrap_or_default();

            if packet_id == current_packet_id + 1 {
                let new_id =
                    if let Some(queue) = REQUEST_QUEUES.lock().unwrap().get_mut(&request_id) {
                        let mut target_id = packet_id + 1;

                        for (id, _) in queue.iter() {
                            if *id != target_id {
                                break;
                            }
                            target_id += 1;
                        }

                        while !queue.is_empty() {
                            if queue[0].0 > target_id {
                                break;
                            }
                            let removed = queue.remove(0).1;
                            data.push_str(&removed);
                        }

                        if queue.is_empty() {
                            REQUEST_QUEUES.lock().unwrap().remove(&request_id);
                        }

                        target_id + 1
                    } else {
                        packet_id
                    };

                REQUEST_PACKET_IDS
                    .lock()
                    .unwrap()
                    .insert(request_id, new_id);
            } else {
                let mut queues = REQUEST_QUEUES.lock().unwrap();
                if let Some(queue) = queues.get_mut(&request_id) {
                    if let Some(pos) = queue.iter().position(|(id, _)| *id > packet_id) {
                        queue.insert(pos, (packet_id, data));
                    } else {
                        queue.push((packet_id, data));
                    }
                } else {
                    queues.insert(request_id, vec![(packet_id, data)]);
                }
                return Ok(());
            }

            let mut data = if let Some(buffer) = REQUEST_BUFFERS.lock().unwrap().get(&request_id) {
                let mut buf = buffer.clone();
                buf.push_str(&data);
                buf
            } else {
                data
            };

            if data.starts_with('{') {
                if let Some(end) = data.chars().position(|c| c == '}') {
                    data.remove(0); // Remove '{'
                    let temp = data.split_off(end - 1);
                    let chunk = data;
                    let mut data = temp;
                    data.remove(0); // Remove '}'
                    if data.is_empty() {
                        REQUEST_BUFFERS.lock().unwrap().remove(&request_id);
                    } else {
                        REQUEST_BUFFERS.lock().unwrap().insert(request_id, data);
                    }
                    match general_purpose::STANDARD.decode(chunk) {
                        Ok(bytes) => Some(TunnelResponse {
                            request_id,
                            packet_id,
                            bytes: Bytes::from(bytes),
                        }),
                        Err(_err) => {
                            warn!("Failed to decode base64 data for request {request_id}");
                            None
                        }
                    }
                } else {
                    REQUEST_BUFFERS.lock().unwrap().insert(request_id, data);
                    None
                }
            } else if data.is_empty() {
                REQUEST_BUFFERS.lock().unwrap().remove(&request_id);
                Some(TunnelResponse {
                    request_id,
                    packet_id,
                    bytes: Bytes::new(),
                })
            } else {
                None
            }
        }
        Message::Binary(bytes) => {
            let data = bytes[12..].to_vec();
            let request_id = usize::from_be_bytes(bytes[..8].try_into().unwrap());
            let packet_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());

            Some(TunnelResponse {
                request_id,
                packet_id,
                bytes: Bytes::from(data),
            })
        }
        Message::Ping(_m) => None,
        Message::Pong(_m) => None,
        Message::Close(_m) => None,
        Message::Frame(_m) => None,
    };

    if let Some(message) = message {
        tx.send(message)?
    }

    Ok(())
}

pub fn start_serverless(client_id: String) -> Receiver<TunnelResponse> {
    start(client_id, serverless_handler)
}

pub fn start<T>(
    client_id: String,
    handler: fn(sender: crossbeam_channel::Sender<T>, m: Message) -> Result<(), SendError<T>>,
) -> Receiver<T>
where
    T: Send + 'static,
{
    let (tx, rx) = bounded(1);

    RT.spawn(async move {
        let mut just_retried = false;

        loop {
            let (txf, rxf) = futures_channel::mpsc::unbounded();

            super::SENDER.lock().unwrap().replace(txf.clone());

            let url = WS_HOST.lock().unwrap().clone().unwrap();
            let mut id = 0;

            debug!("Connecting to websocket...");
            match connect_async(format!("{}?clientId={}", url, client_id)).await {
                Ok((ws_stream, _)) => {
                    just_retried = false;
                    id += 1;
                    let ws_connection_id = id;
                    debug!("WebSocket handshake has been successfully completed");

                    let (write, read) = ws_stream.split();

                    let stdin_to_ws = rxf.map(Ok).forward(write);

                    let ws_to_stdout = read.for_each(|m| async {
                        let m = match m {
                            Ok(m) => m,
                            Err(e) => {
                                error!("Send Loop error: {:?}", e);
                                return;
                            }
                        };

                        if let Err(e) = handler(tx.clone(), m) {
                            error!("Handler Send Loop error {ws_connection_id}: {e:?}");
                        }
                    });

                    pin_mut!(stdin_to_ws, ws_to_stdout);
                    future::select(stdin_to_ws, ws_to_stdout).await;
                    info!("Websocket connection closed");
                }
                Err(err) => match err {
                    Error::Http(response) => {
                        let body = std::str::from_utf8(response.body().as_ref().unwrap()).unwrap();
                        error!("body: {}", body);
                    }
                    _ => error!("Failed to connect to websocket server: {err:?}"),
                },
            }

            if just_retried {
                thread::sleep(Duration::from_millis(5000));
            } else {
                just_retried = true;
            }
        }
    });

    rx.clone()
}

#[derive(Debug, Error)]
pub enum SendBytesError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

pub fn send_bytes(bytes: impl Into<Vec<u8>>) -> Result<(), SendBytesError> {
    if let Some(sender) = SENDER.lock().unwrap().as_ref() {
        sender
            .unbounded_send(Message::Binary(bytes.into()))
            .map_err(|err| SendBytesError::Unknown(format!("Failed to send message: {err:?}")))?;
    } else {
        return Err(SendBytesError::Unknown("Failed to get sender2".into()));
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum SendMessageError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

pub fn send_message(message: impl Into<String>) -> Result<(), SendMessageError> {
    if let Some(sender) = SENDER.lock().unwrap().as_ref() {
        sender
            .unbounded_send(Message::Text(message.into()))
            .map_err(|err| SendMessageError::Unknown(format!("Failed to send message: {err:?}")))?;
    } else {
        return Err(SendMessageError::Unknown("Failed to get sender3".into()));
    }

    Ok(())
}
