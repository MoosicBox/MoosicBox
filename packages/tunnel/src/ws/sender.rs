use std::{thread, time::Duration};

use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, SendError};
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
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

fn message_handler(
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

pub fn start(client_id: String) -> Receiver<TunnelMessage> {
    start_tunnel(client_id, message_handler)
}

pub struct TunnelResponse {
    pub request_id: usize,
    pub packet_id: u32,
    pub bytes: Bytes,
}

pub fn start_tunnel<T>(
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
