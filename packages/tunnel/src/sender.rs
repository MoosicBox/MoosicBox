use std::fs::File;
use std::io::Cursor;
use std::sync::Mutex;
use std::{thread, time::Duration};

#[cfg(feature = "base64")]
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, SendError};
use futures_channel::mpsc::UnboundedSender;
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use moosicbox_core::app::Db;
use moosicbox_files::files::track::{get_track_info, get_track_source, TrackSource};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_aux::prelude::*;
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;
use tokio::runtime::{self, Runtime};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{protocol::frame::Frame, Error, Message},
};

use crate::tunnel::TunnelEncoding;

pub static SENDER: Lazy<Mutex<Option<UnboundedSender<Message>>>> = Lazy::new(|| Mutex::new(None));
static WS_HOST: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

pub fn init_host(
    host: String,
) -> std::result::Result<
    (),
    std::sync::PoisonError<
        std::sync::MutexGuard<'static, std::option::Option<std::string::String>>,
    >,
> {
    WS_HOST.lock()?.replace(host);
    Ok(())
}

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

pub fn start_tunnel<T>(
    client_id: String,
    handler: fn(sender: crossbeam_channel::Sender<T>, m: Message) -> Result<(), SendError<T>>,
) -> Receiver<T>
where
    T: Send + 'static,
{
    let (tx, rx) = bounded(1024);

    RT.spawn(async move {
        let mut just_retried = false;

        loop {
            let (txf, rxf) = futures_channel::mpsc::unbounded();

            SENDER.lock().unwrap().replace(txf.clone());

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

                    let ws_writer = rxf.map(Ok).forward(write);

                    let ws_reader = read.for_each(|m| async {
                        let m = match m {
                            Ok(m) => m,
                            Err(e) => {
                                error!("Send Loop error: {:?}", e);
                                return;
                            }
                        };

                        if let Err(e) = handler(tx.clone(), m) {
                            error!("Handler Send Loop error {ws_connection_id}: {e:?}");
                            txf.unbounded_send(Message::Close(None)).unwrap();
                        }
                    });

                    pin_mut!(ws_writer, ws_reader);
                    future::select(ws_writer, ws_reader).await;
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

#[derive(Debug, Error)]
pub enum TunnelRequestError {
    #[error("Invalid Query: {0}")]
    InvalidQuery(String),
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Head,
    Get,
    Post,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackInfoQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
}

fn send(request_id: usize, reader: impl std::io::Read, encoding: TunnelEncoding) {
    match encoding {
        TunnelEncoding::Binary => send_binary(request_id, reader),
        #[cfg(feature = "base64")]
        TunnelEncoding::Base64 => send_base64(request_id, reader),
    }
}

fn send_binary(request_id: usize, mut reader: impl std::io::Read) {
    let buf_size = 1024 * 32;

    let mut bytes_read = 0_usize;
    let mut packet_id = 0_u32;

    loop {
        let mut buf = vec![0_u8; buf_size];
        let mut offset = 0_usize;

        let id_bytes = request_id.to_be_bytes();
        let len = id_bytes.len();
        buf[..len].copy_from_slice(&id_bytes);
        offset += len;

        let packet_id_bytes = packet_id.to_be_bytes();
        let len = packet_id_bytes.len();
        buf[offset..(offset + len)].copy_from_slice(&packet_id_bytes);
        offset += len;

        match reader.read(&mut buf[offset..]) {
            Ok(size) => {
                packet_id += 1;
                bytes_read += size;
                debug!("Read {} bytes", bytes_read);
                let bytes = &buf[..(size + offset)];
                send_bytes(bytes).unwrap();
                if size == 0 {
                    break;
                }
            }
            Err(_err) => break,
        }
    }
}

#[cfg(feature = "base64")]
fn send_base64(request_id: usize, mut reader: impl std::io::Read) {
    use std::cmp::min;

    let buf_size = 1024 * 32;
    let mut overflow_buf = "".to_owned();

    let mut bytes_read = 0_usize;
    let mut packet_id = 0_u32;

    loop {
        let mut buf = vec![0_u8; buf_size];
        match reader.read(&mut buf) {
            Ok(size) => {
                packet_id += 1;
                bytes_read += size;
                debug!("Read {} bytes", bytes_read);
                let bytes = &buf[..size];
                let prefix = format!("{request_id}|{packet_id}|");
                let mut base64 = general_purpose::STANDARD.encode(bytes);
                base64.insert(0, '{');
                base64.push('}');
                if !overflow_buf.is_empty() {
                    overflow_buf.push_str(&base64);
                    base64 = overflow_buf;
                    overflow_buf = "".to_owned();
                }
                let end = min(base64.len(), buf_size - prefix.len());
                let data = &base64[..end];
                overflow_buf.push_str(&base64[end..]);
                send_message(format!("{prefix}{data}")).unwrap();

                if size == 0 {
                    while !overflow_buf.is_empty() {
                        let base64 = overflow_buf;
                        overflow_buf = "".to_owned();
                        let end = min(base64.len(), buf_size - prefix.len());
                        let data = &base64[..end];
                        overflow_buf.push_str(&base64[end..]);
                        packet_id += 1;
                        let prefix = format!("{request_id}|{packet_id}|");
                        send_message(format!("{prefix}{data}")).unwrap();
                    }

                    packet_id += 1;
                    let prefix = format!("{request_id}|{packet_id}|");
                    send_message(prefix).unwrap();
                }
                if size == 0 {
                    break;
                }
            }
            Err(_err) => break,
        }
    }
}

pub async fn tunnel_request(
    db: &Db,
    request_id: usize,
    method: Method,
    path: String,
    query: Value,
    _payload: Value,
    encoding: TunnelEncoding,
) -> Result<(), TunnelRequestError> {
    match path.as_str() {
        "track" => match method {
            Method::Get => {
                let query = serde_json::from_value::<GetTrackQuery>(query)
                    .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                if let Ok(TrackSource::LocalFilePath(path)) =
                    get_track_source(query.track_id, db.clone()).await
                {
                    send(request_id, File::open(path).unwrap(), encoding);
                }

                Ok(())
            }
            _ => todo!(),
        },
        "track/info" => match method {
            Method::Get => {
                let query = serde_json::from_value::<GetTrackInfoQuery>(query)
                    .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                if let Ok(track_info) = get_track_info(query.track_id, db.clone()).await {
                    let mut bytes: Vec<u8> = Vec::new();
                    serde_json::to_writer(&mut bytes, &track_info).unwrap();
                    send(request_id, Cursor::new(bytes), encoding);
                }

                Ok(())
            }
            _ => todo!(),
        },
        "albums" => Ok(()),
        _ => Ok(()),
    }
}
