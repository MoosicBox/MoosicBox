use std::{thread, time::Duration};

use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver};
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error};
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

pub fn start() -> (Receiver<()>, Receiver<TunnelMessage>) {
    let (ready, on_ready) = bounded(1);
    let (tx, rx) = bounded(1);

    RT.spawn(async move {
        loop {
            let (txf, rxf) = futures_channel::mpsc::unbounded();

            super::SENDER.lock().unwrap().replace(txf.clone());
            ready.send(()).unwrap();

            let url = WS_HOST.lock().unwrap().clone().unwrap();

            match connect_async(url).await {
                Ok((ws_stream, _)) => {
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

                        debug!("Message from tunnel ws server: {m:?}");
                        tx.send(match m {
                            Message::Text(m) => TunnelMessage::Text(m),
                            Message::Binary(m) => TunnelMessage::Binary(Bytes::from(m)),
                            Message::Ping(m) => TunnelMessage::Ping(m),
                            Message::Pong(m) => TunnelMessage::Pong(m),
                            Message::Close(_m) => TunnelMessage::Close,
                            Message::Frame(m) => TunnelMessage::Frame(m),
                        })
                        .unwrap();
                    });

                    txf.unbounded_send(Message::Text("{\"type\":\"GET_CONNECTION_ID\"}".into()))
                        .unwrap();

                    pin_mut!(stdin_to_ws, ws_to_stdout);
                    future::select(stdin_to_ws, ws_to_stdout).await;
                }
                Err(err) => match err {
                    Error::Http(response) => {
                        let body = std::str::from_utf8(response.body().as_ref().unwrap()).unwrap();
                        error!("body: {}", body);
                    }
                    _ => error!("Failed to connect to websocket server: {err:?}"),
                },
            }
            thread::sleep(Duration::from_millis(5000));
        }
    });

    (on_ready, rx)
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
