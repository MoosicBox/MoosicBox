use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[cfg(feature = "base64")]
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use futures_channel::mpsc::UnboundedSender;
use futures_util::future::ready;
use futures_util::{future, pin_mut, Future, Stream, StreamExt};
use lazy_static::lazy_static;
use moosicbox_auth::FetchSignatureError;
use moosicbox_core::sqlite::models::{AlbumId, ApiSource};
use moosicbox_core::types::AudioFormat;
use moosicbox_database::Database;
use moosicbox_env_utils::default_env_usize;
use moosicbox_files::api::AlbumCoverQuery;
use moosicbox_files::files::album::{get_album_cover, AlbumCoverError, AlbumCoverSource};
use moosicbox_files::files::track::{
    audio_format_to_content_type, get_track_id_source, get_track_info, TrackSource,
};
use moosicbox_files::range::{parse_ranges, Range};
use moosicbox_stream_utils::ByteWriter;
use moosicbox_symphonia_player::media_sources::remote_bytestream::RemoteByteStream;
use moosicbox_symphonia_player::output::AudioOutputHandler;
use moosicbox_symphonia_player::play_media_source;
use moosicbox_tunnel::{Method, TunnelEncoding, TunnelWsResponse};
use moosicbox_ws::{WebsocketContext, WebsocketSendError, WebsocketSender};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use regex::Regex;
use serde_json::Value;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;
use thiserror::Error;
use tokio::runtime::{self, Runtime};
use tokio::select;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error, Message},
};
use tokio_util::sync::CancellationToken;

use super::{
    GetTrackInfoQuery, GetTrackQuery, SendBytesError, SendMessageError, TunnelMessage,
    TunnelRequestError,
};
use crate::websocket_sender::TunnelWebsocketSender;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(64)
        .build()
        .unwrap();
}

#[derive(Debug, Error)]
pub enum CloseError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

#[derive(Clone)]
pub struct TunnelSenderHandle {
    sender: Arc<RwLock<Option<UnboundedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
}

impl TunnelSenderHandle {
    pub async fn close(&self) -> Result<(), CloseError> {
        self.cancellation_token.cancel();

        Ok(())
    }
}

#[allow(unused)]
fn wrap_to_500<E: std::error::Error + 'static>(e: E) -> TunnelRequestError {
    TunnelRequestError::InternalServerError(Box::new(e))
}

impl WebsocketSender for TunnelSenderHandle {
    fn send(&self, conn_id: &str, data: &str) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: None,
                    to_connection_ids: Some(vec![conn_id.parse::<usize>()?]),
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    fn send_all(&self, data: &str) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: None,
                    to_connection_ids: None,
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }

    fn send_all_except(
        &self,
        conn_id: &str,
        data: &str,
    ) -> Result<(), moosicbox_ws::WebsocketSendError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage::Ws(TunnelResponseWs {
                    message: data.into(),
                    exclude_connection_ids: Some(vec![conn_id.parse::<usize>()?]),
                    to_connection_ids: None,
                }))
                .map_err(|e| WebsocketSendError::Unknown(e.to_string()))?;
        }
        Ok(())
    }
}

pub enum TunnelResponseMessage {
    Packet(TunnelResponsePacket),
    Ws(TunnelResponseWs),
}

pub struct TunnelResponsePacket {
    pub request_id: usize,
    pub packet_id: u32,
    pub message: Message,
    pub broadcast: bool,
    pub except_id: Option<usize>,
    pub only_id: Option<usize>,
}

pub struct TunnelResponseWs {
    pub message: Message,
    pub exclude_connection_ids: Option<Vec<usize>>,
    pub to_connection_ids: Option<Vec<usize>>,
}

#[derive(Clone)]
pub struct TunnelSender {
    id: usize,
    host: String,
    url: String,
    client_id: String,
    access_token: String,
    sender: Arc<RwLock<Option<UnboundedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
    abort_request_tokens: Arc<RwLock<HashMap<usize, CancellationToken>>>,
}

static BINARY_REQUEST_BUFFER_OFFSET: Lazy<usize> = Lazy::new(|| {
    std::mem::size_of::<usize>() + // request_id
    std::mem::size_of::<u32>() + // packet_id
    std::mem::size_of::<u8>() // last
});

static DEFAULT_WS_MAX_PACKET_SIZE: usize = 1024 * 64;
static WS_MAX_PACKET_SIZE: usize =
    default_env_usize!("WS_MAX_PACKET_SIZE", DEFAULT_WS_MAX_PACKET_SIZE);

impl TunnelSender {
    pub fn new(
        host: String,
        url: String,
        client_id: String,
        access_token: String,
    ) -> (Self, TunnelSenderHandle) {
        let sender = Arc::new(RwLock::new(None));
        let cancellation_token = CancellationToken::new();
        let id = thread_rng().gen::<usize>();
        let handle = TunnelSenderHandle {
            sender: sender.clone(),
            cancellation_token: cancellation_token.clone(),
        };

        (
            Self {
                id,
                host,
                url,
                client_id,
                access_token,
                sender: sender.clone(),
                cancellation_token: cancellation_token.clone(),
                abort_request_tokens: Arc::new(RwLock::new(HashMap::new())),
            },
            handle,
        )
    }

    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    async fn message_handler(
        tx: Sender<TunnelMessage>,
        m: Message,
    ) -> Result<(), SendError<TunnelMessage>> {
        log::trace!("Message from tunnel ws server: {m:?}");
        tx.send(match m {
            Message::Text(m) => TunnelMessage::Text(m),
            Message::Binary(m) => TunnelMessage::Binary(Bytes::from(m)),
            Message::Ping(m) => TunnelMessage::Ping(m),
            Message::Pong(m) => TunnelMessage::Pong(m),
            Message::Close(_m) => TunnelMessage::Close,
            Message::Frame(m) => TunnelMessage::Frame(m),
        })
        .await
    }

    pub fn start(&mut self) -> Receiver<TunnelMessage> {
        self.start_tunnel(Self::message_handler)
    }

    fn is_request_aborted(
        request_id: usize,
        tokens: Arc<RwLock<HashMap<usize, CancellationToken>>>,
    ) -> bool {
        if let Some(token) = tokens.read().unwrap().get(&request_id) {
            return token.is_cancelled();
        }
        false
    }

    fn start_tunnel<T, O>(&mut self, handler: fn(sender: Sender<T>, m: Message) -> O) -> Receiver<T>
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

        RT.spawn(async move {
            let mut just_retried = false;
            log::debug!("Fetching signature token...");
            let token = loop {
                if cancellation_token.is_cancelled() {
                    log::debug!("Closing tunnel");
                    return;
                }
                match moosicbox_auth::fetch_signature_token(&host, &client_id, &access_token).await
                {
                    Ok(Some(token)) => break token,
                    Err(FetchSignatureError::Unauthorized) => {
                        log::error!("Unauthorized response from fetch_signature_token");
                    },
                    Err(err) => {
                        log::error!("Failed to fetch signature token: {err:?}");
                    },
                    _ => {}
                }

                select!(
                    _ = sleep(Duration::from_millis(5000)) => {}
                    _ = cancellation_token.cancelled() => {
                        log::debug!("Cancelling retry")
                    }
                );
            };

            loop {
                let close_token = CancellationToken::new();

                if cancellation_token.is_cancelled() {
                    log::debug!("Closing tunnel");
                    break;
                }
                let (txf, rxf) = futures_channel::mpsc::unbounded();

                sender_arc.write().unwrap().replace(txf.clone());

                log::debug!("Connecting to websocket...");
                match connect_async(format!(
                    "{}?clientId={}&sender=true&signature={token}",
                    url, client_id
                ))
                .await
                {
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
                                        if Self::is_request_aborted(packet.request_id, abort_request_tokens.clone()) {
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
                                            log::debug!(
                                                "Sending ws message to={:?} exclude={:?} size={}",
                                                ws.to_connection_ids,
                                                ws.exclude_connection_ids,
                                                text.len()
                                            );
                                            serde_json::from_str(&text).and_then(|value: Value| {
                                                serde_json::to_string(&TunnelWsResponse {
                                                    request_id: 0,
                                                    body: value,
                                                    exclude_connection_ids: ws.exclude_connection_ids,
                                                    to_connection_ids: ws.to_connection_ids,
                                                }).map(Message::Text)
                                            }).map_err(|e| {
                                                log::error!("Serde error occurred: {e:?}");
                                                tokio_tungstenite::tungstenite::Error::AlreadyClosed
                                            })
                                        } else {
                                            Ok(ws.message)
                                        }
                                    }
                                }
                            })
                            .forward(write);

                        let ws_reader = read.for_each(|m| async {
                            let m = match m {
                                Ok(m) => m,
                                Err(e) => {
                                    log::error!("Send Loop error: {:?}", e);
                                    close_token.cancel();
                                    return;
                                }
                            };

                            if let Err(e) = handler(tx.clone(), m).await {
                                log::error!("Handler Send Loop error: {e:?}");
                                close_token.cancel();
                            }
                        });

                        pin_mut!(ws_writer, ws_reader);
                        select!(
                            _ = close_token.cancelled() => {}
                            _ = cancellation_token.cancelled() => {}
                            _ = future::select(ws_writer, ws_reader) => {}
                        );
                        log::info!("WebSocket connection closed");
                    }
                    Err(err) => match err {
                        Error::Http(response) => {
                            if let Ok(body) =
                                std::str::from_utf8(response.body().as_ref().unwrap_or(&vec![]))
                            {
                                log::error!("body: {}", body);
                            } else {
                                log::error!("body: (unable to get body)");
                            }
                        }
                        _ => log::error!("Failed to connect to websocket server: {err:?}"),
                    },
                }

                if just_retried {
                    select!(
                        _ = sleep(Duration::from_millis(5000)) => {}
                        _ = cancellation_token.cancelled() => {
                            log::debug!("Cancelling retry")
                        }
                    );
                } else {
                    just_retried = true;
                }
            }
        });

        rx
    }

    pub fn send_bytes(
        &self,
        request_id: usize,
        packet_id: u32,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), SendBytesError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage::Packet(TunnelResponsePacket {
                    request_id,
                    packet_id,
                    broadcast: true,
                    except_id: None,
                    only_id: None,
                    message: Message::Binary(bytes.into()),
                }))
                .map_err(|err| SendBytesError::Unknown(format!("Failed to send_bytes: {err:?}")))?;
        } else {
            return Err(SendBytesError::Unknown(
                "Failed to get sender for send_bytes".into(),
            ));
        }

        Ok(())
    }

    pub fn send_message(
        &self,
        request_id: usize,
        packet_id: u32,
        message: impl Into<String>,
    ) -> Result<(), SendMessageError> {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage::Packet(TunnelResponsePacket {
                    request_id,
                    packet_id,
                    broadcast: true,
                    except_id: None,
                    only_id: None,
                    message: Message::Text(message.into()),
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

    fn send(
        &self,
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        reader: impl std::io::Read,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        match encoding {
            TunnelEncoding::Binary => self.send_binary(request_id, status, headers, reader),
            #[cfg(feature = "base64")]
            TunnelEncoding::Base64 => self.send_base64(request_id, status, headers, reader),
        }
    }

    async fn send_stream<E: std::error::Error + Sized>(
        &self,
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        ranges: Option<Vec<Range>>,
        stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
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
        request_id: usize,
        packet_id: u32,
        last: bool,
        status: u16,
        headers: &HashMap<String, String>,
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

        let last_bytes = if last { 1u8 } else { 0u8 }.to_be_bytes();
        let len = last_bytes.len();
        buf[offset..(offset + len)].copy_from_slice(&last_bytes);
        offset += len;

        assert!(
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
            let headers_len = headers_bytes.len() as u32;
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

    async fn send_binary_stream<E: std::error::Error + Sized>(
        &self,
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        ranges: Option<Vec<Range>>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
    ) -> Result<(), TunnelRequestError> {
        let mut bytes_read = 0_usize;
        let mut bytes_consumed = 0_usize;
        let mut packet_id = 1_u32;
        let mut left_over: Option<Vec<u8>> = None;
        let mut last = false;

        while !last {
            if Self::is_request_aborted(request_id, self.abort_request_tokens.clone()) {
                log::debug!("Aborting send_binary_stream");
                break;
            }
            let mut buf = vec![0_u8; WS_MAX_PACKET_SIZE];
            let mut header_offset = Self::init_binary_request_buffer(
                request_id, packet_id, false, status, &headers, &mut buf,
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
                            request_id, packet_id, false, status, &headers, &mut buf,
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
        !range.start.is_some_and(|start| start >= packet_end)
            && !range.end.is_some_and(|end| end < packet_start)
    }

    fn send_binary(
        &self,
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        mut reader: impl std::io::Read,
    ) -> Result<(), TunnelRequestError> {
        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;
        let mut last = false;

        while !last {
            if Self::is_request_aborted(request_id, self.abort_request_tokens.clone()) {
                log::debug!("Aborting send_binary");
                break;
            }
            packet_id += 1;
            let mut buf = vec![0_u8; WS_MAX_PACKET_SIZE];
            let offset = Self::init_binary_request_buffer(
                request_id, packet_id, false, status, &headers, &mut buf,
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

        Ok(())
    }

    #[cfg(feature = "base64")]
    fn init_base64_request_buffer(
        request_id: usize,
        packet_id: u32,
        status: u16,
        headers: &HashMap<String, String>,
        buf: &mut String,
        overflow_buf: &mut String,
    ) -> String {
        if !overflow_buf.is_empty() {
            overflow_buf.push_str(buf);
            *buf = overflow_buf.to_string();
            "".clone_into(overflow_buf);
        }

        let mut prefix = format!("{request_id}|{packet_id}|");
        if packet_id == 1 {
            prefix.push_str(&status.to_string());
            let mut headers_base64 =
                general_purpose::STANDARD.encode(serde_json::to_string(&headers).unwrap().clone());
            headers_base64.insert(0, '{');
            headers_base64.push('}');
            prefix.push_str(&headers_base64);
        }

        prefix
    }

    #[cfg(feature = "base64")]
    fn send_base64(
        &self,
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        mut reader: impl std::io::Read,
    ) -> Result<(), TunnelRequestError> {
        use std::cmp::min;

        let buf_size = 1024 * 32;
        let mut overflow_buf = "".to_owned();

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;

        loop {
            if Self::is_request_aborted(request_id, self.abort_request_tokens.clone()) {
                log::debug!("Aborting send_base64");
                break Ok(());
            }
            let mut buf = vec![0_u8; buf_size];
            match reader.read(&mut buf) {
                Ok(size) => {
                    packet_id += 1;
                    bytes_read += size;
                    log::debug!("Read {} bytes", bytes_read);
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
                        overflow_buf = "".to_string();
                    }
                    let end = min(base64.len(), buf_size - prefix.len());
                    let data = &base64[..end];
                    overflow_buf.push_str(&base64[end..]);
                    self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                        .map_err(wrap_to_500)?;

                    if size == 0 {
                        while !overflow_buf.is_empty() {
                            let base64 = overflow_buf;
                            overflow_buf = "".to_string();
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
        request_id: usize,
        status: u16,
        headers: HashMap<String, String>,
        ranges: Option<Vec<Range>>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
    ) -> Result<(), TunnelRequestError> {
        if ranges.is_some() {
            todo!("Byte ranges for base64 not implemented");
        }

        use std::cmp::min;

        let buf_size = 1024 * 32;
        let mut overflow_buf = "".to_owned();

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;

        loop {
            if Self::is_request_aborted(request_id, self.abort_request_tokens.clone()) {
                log::debug!("Aborting send_base64_stream");
                break Ok(());
            }
            packet_id += 1;

            let mut buf = "".to_owned();

            let prefix = Self::init_base64_request_buffer(
                request_id,
                packet_id,
                status,
                &headers,
                &mut buf,
                &mut overflow_buf,
            );
            let size_offset = prefix.len();

            loop {
                match stream.next().await {
                    Some(Ok(data)) => {
                        let size = data.len();
                        bytes_read += size;
                        log::debug!("Read {} bytes", bytes_read);
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
        request_id: usize,
        method: Method,
        path: String,
        query: Value,
        payload: Option<Value>,
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
            query_string.insert(0, '?')
        }

        let url = format!("{host}/{path}{query_string}");

        self.proxy_request(&url, request_id, method, payload, encoding)
            .await
    }

    async fn proxy_request(
        &self,
        url: &str,
        request_id: usize,
        method: Method,
        payload: Option<Value>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        let response = self.http_request(url, method, payload, true).await?;

        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_str().unwrap().to_string()))
            .collect();

        self.send_stream(
            request_id,
            response.status().as_u16(),
            headers,
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
        user_agent_header: bool,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let client = reqwest::Client::new();

        let mut builder = match method {
            Method::Post => client.post(url),
            Method::Get => client.get(url),
            Method::Head => client.head(url),
            Method::Put => client.put(url),
            Method::Patch => client.patch(url),
            Method::Delete => client.delete(url),
            Method::Options => client.request(reqwest::Method::OPTIONS, url),
        };

        if user_agent_header {
            builder = builder.header("user-agent", "MOOSICBOX_TUNNEL");
        }

        if let Some(body) = payload {
            builder = builder.json(&body);
        }

        builder.send().await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tunnel_request(
        &self,
        database: Arc<Box<dyn Database>>,
        service_port: u16,
        request_id: usize,
        method: Method,
        path: String,
        query: Value,
        payload: Option<Value>,
        headers: Option<Value>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        let abort_token = CancellationToken::new();

        {
            self.abort_request_tokens
                .write()
                .unwrap()
                .insert(request_id, abort_token.clone());
        }

        match path.to_lowercase().as_str() {
            "track" => match method {
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
                            range
                                .clone()
                                .strip_prefix("bytes=")
                                .map(|s| s.to_string())
                                .ok_or(TunnelRequestError::BadRequest(format!(
                                    "Invalid bytes range '{range:?}'"
                                )))
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

                    let mut response_headers = HashMap::new();
                    response_headers.insert("accept-ranges".to_string(), "bytes".to_string());

                    match get_track_id_source(
                        query.track_id,
                        &**database,
                        query.quality,
                        query.source,
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
                                #[cfg(feature = "aac")]
                                Some(AudioFormat::Aac) => {
                                    self.send_stream(request_id, 200, response_headers, ranges,
                                        moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac_stream(
                                            path,
                                        ),
                                        encoding,
                                    ).await?;
                                }
                                #[cfg(feature = "mp3")]
                                Some(AudioFormat::Mp3) => {
                                    self.send_stream(request_id, 200, response_headers, ranges,
                                        moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3_stream(
                                            path,
                                        ),
                                        encoding,
                                    ).await?;
                                }
                                #[cfg(feature = "opus")]
                                Some(AudioFormat::Opus) => {
                                    self.send_stream(request_id, 200, response_headers, ranges,
                                        moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus_stream(
                                            path,
                                        ),
                                        encoding,
                                    ).await?;
                                }
                                _ => {
                                    self.send(
                                        request_id,
                                        200,
                                        response_headers,
                                        File::open(path)?,
                                        encoding,
                                    )?;
                                }
                            }
                        }
                        Ok(TrackSource::Tidal { url, .. }) | Ok(TrackSource::Qobuz { url, .. }) => {
                            let writer = ByteWriter::default();
                            let stream = writer.stream();

                            RT.spawn(async move {
                                let mut audio_output_handler = AudioOutputHandler::new();

                                let format = match query.format {
                                    #[cfg(feature = "aac")]
                                    None | Some(AudioFormat::Source) => AudioFormat::Aac,
                                    #[cfg(all(not(feature = "aac"), feature = "mp3"))]
                                    None | Some(AudioFormat::Source) => AudioFormat::Mp3,
                                    #[cfg(all(not(feature = "aac"), not(feature = "mp3"), feature = "opus"))]
                                    None | Some(AudioFormat::Source) => AudioFormat::Opus,
                                    #[cfg(all(not(feature = "aac"), not(feature = "mp3"), not(feature = "opus")))]
                                    None | Some(AudioFormat::Source) => panic!("Audio format is unsupported for Tidal"),
                                    #[cfg(feature = "flac")]
                                    Some(AudioFormat::Flac) => panic!("FLAC audio format is unsupported for Tidal"),
                                    #[allow(unreachable_patterns)]
                                    Some(format) => format
                                };

                                log::debug!("Sending audio stream with format: {format:?}");

                                match format {
                                    #[cfg(feature = "aac")]
                                    AudioFormat::Aac => {
                                        use moosicbox_symphonia_player::output::encoder::aac::encoder::AacEncoder;
                                        log::debug!("Using AAC encoder for output");
                                        audio_output_handler = audio_output_handler.with_output(Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                AacEncoder::with_writer(writer.clone())
                                                    .open(spec, duration),
                                            ))
                                        }));
                                    }
                                    #[cfg(feature = "mp3")]
                                    AudioFormat::Mp3 => {
                                        use moosicbox_symphonia_player::output::encoder::mp3::encoder::Mp3Encoder;
                                        log::debug!("Using MP3 encoder for output");
                                        audio_output_handler = audio_output_handler.with_output(Box::new(move |spec, duration| {
                                            Ok(Box::new(
                                                Mp3Encoder::with_writer(writer.clone()).open(spec, duration)
                                            ))
                                        }));
                                    }
                                    #[cfg(feature = "opus")]
                                    AudioFormat::Opus => {
                                        use moosicbox_symphonia_player::output::encoder::opus::encoder::OpusEncoder;
                                        log::debug!("Using OPUS encoder for output");
                                        audio_output_handler = audio_output_handler.with_output(Box::new(move |spec, duration| {
                                            Ok(Box::new(OpusEncoder::with_writer(writer.clone()) .open(spec, duration)))
                                        }));
                                    }
                                    _ => {}
                                }

                                log::debug!("Creating RemoteByteStream with url={url}");
                                let source = Box::new(RemoteByteStream::new(
                                    url,
                                    None,
                                    true,
                                    #[cfg(feature = "flac")]
                                    query.format.is_some_and(|format|
                                        format == AudioFormat::Flac),
                                    #[cfg(not(feature = "flac"))]
                                    false,
                                    CancellationToken::new(),
                                ));

                                if let Err(err) = play_media_source(
                                    MediaSourceStream::new(source, Default::default()),
                                    &Hint::new(),
                                    &mut audio_output_handler,
                                    true,
                                    true,
                                    None,
                                    None,
                                ) {
                                    log::error!("Failed to encode to {:?}: {err:?}", query.format);
                                }
                            });

                            self.send_stream(
                                request_id,
                                200,
                                response_headers,
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
                        encoding,
                    )
                    .await
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            "track/info" => match method {
                Method::Get => {
                    let query = serde_json::from_value::<GetTrackInfoQuery>(query)
                        .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "application/json".to_string());

                    if let Ok(track_info) = get_track_info(query.track_id as u64, &**database).await
                    {
                        let mut bytes: Vec<u8> = Vec::new();
                        serde_json::to_writer(&mut bytes, &track_info)?;
                        self.send(request_id, 200, headers, Cursor::new(bytes), encoding)?;
                    }

                    Ok(())
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            _ => {
                let re = Regex::new(r"^albums/(\d+)/(\d+)x(\d+)$")?;
                if let Some(caps) = re.captures(&path) {
                    match method {
                        Method::Get => {
                            let query = serde_json::from_value::<AlbumCoverQuery>(query)
                                .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                            let album_id_string = caps
                                .get(1)
                                .ok_or(TunnelRequestError::BadRequest("Invalid album_id".into()))?
                                .as_str();

                            let album_id = match query.source.unwrap_or(ApiSource::Library) {
                                ApiSource::Library => {
                                    album_id_string.parse::<i32>().map(AlbumId::Library)
                                }
                                ApiSource::Tidal => {
                                    album_id_string.parse::<u64>().map(AlbumId::Tidal)
                                }
                                ApiSource::Qobuz => Ok(AlbumId::Qobuz(album_id_string.to_string())),
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

                            match get_album_cover(
                                album_id,
                                &**database,
                                Some(std::cmp::max(width, height)),
                            )
                            .await
                            .map_err(|e| TunnelRequestError::Request(e.to_string()))?
                            {
                                AlbumCoverSource::LocalFilePath(path) => {
                                    let mut headers = HashMap::new();
                                    let resized = {
                                        use moosicbox_image::{
                                            image::try_resize_local_file, Encoding,
                                        };
                                        if let Some(resized) = try_resize_local_file(
                                            width,
                                            height,
                                            &path,
                                            Encoding::Webp,
                                            80,
                                        )
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
                                            try_resize_local_file(
                                                width,
                                                height,
                                                &path,
                                                Encoding::Jpeg,
                                                80,
                                            )
                                            .map_err(|e| {
                                                TunnelRequestError::InternalServerError(Box::new(
                                                    AlbumCoverError::File(
                                                        path.clone(),
                                                        e.to_string(),
                                                    ),
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
                                    self.send(
                                        request_id,
                                        200,
                                        headers,
                                        Cursor::new(resized),
                                        encoding,
                                    )?;
                                }
                            }

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
                        encoding,
                    )
                    .await?;

                    Ok(())
                }
            }
        }
    }

    pub async fn ws_request(
        &self,
        db: &dyn Database,
        conn_id: usize,
        request_id: usize,
        value: Value,
        sender: impl WebsocketSender,
    ) -> Result<(), TunnelRequestError> {
        let context = WebsocketContext {
            connection_id: conn_id.to_string(),
            ..Default::default()
        };
        let packet_id = 1_u32;
        log::debug!("Processing tunnel ws request request_id={request_id} packet_id={packet_id} conn_id={conn_id}");
        let sender = TunnelWebsocketSender {
            id: self.id,
            propagate_id: conn_id,
            packet_id,
            request_id,
            root_sender: sender,
            tunnel_sender: self.sender.read().unwrap().clone().unwrap(),
        };
        moosicbox_ws::process_message(db, value, context, &sender).await?;
        log::debug!("Processed tunnel ws request {request_id} {packet_id}");
        Ok(())
    }

    pub fn abort_request(&self, request_id: usize) {
        if let Some(token) = self.abort_request_tokens.read().unwrap().get(&request_id) {
            token.cancel();
        }
    }
}
