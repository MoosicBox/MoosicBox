use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "base64")]
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use futures_channel::mpsc::UnboundedSender;
use futures_util::{future, pin_mut, Future, Stream, StreamExt};
use lazy_static::lazy_static;
use moosicbox_core::app::Db;
use moosicbox_files::api::AudioFormat;
use moosicbox_files::files::track::{get_track_info, get_track_source, TrackSource};
use moosicbox_ws::api::{WebsocketContext, WebsocketSender};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde_json::Value;
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

use crate::sender::tunnel_websocket_sender::TunnelWebsocketSender;
use crate::tunnel::{Method, TunnelEncoding};

use super::{
    GetTrackInfoQuery, GetTrackQuery, SendBytesError, SendMessageError, TunnelMessage,
    TunnelRequestError,
};

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
    _sender: Arc<Mutex<Option<UnboundedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
}

impl TunnelSenderHandle {
    pub async fn close(&self) -> Result<(), CloseError> {
        self.cancellation_token.cancel();

        Ok(())
    }
}

pub struct TunnelResponseMessage {
    pub request_id: usize,
    pub packet_id: u32,
    pub message: Message,
}

#[derive(Clone)]
pub struct TunnelSender {
    id: usize,
    host: String,
    url: String,
    client_id: String,
    access_token: String,
    sender: Arc<Mutex<Option<UnboundedSender<TunnelResponseMessage>>>>,
    cancellation_token: CancellationToken,
}

static BINARY_REQUEST_BUFFER_OFFSET: Lazy<usize> = Lazy::new(|| {
    std::mem::size_of::<usize>() + // request_id
    std::mem::size_of::<u32>() + // packet_id
    std::mem::size_of::<u8>() // last
});

impl TunnelSender {
    pub fn new(
        host: String,
        url: String,
        client_id: String,
        access_token: String,
    ) -> (Self, TunnelSenderHandle) {
        let sender = Arc::new(Mutex::new(None));
        let cancellation_token = CancellationToken::new();
        let id = thread_rng().gen::<usize>();
        let handle = TunnelSenderHandle {
            _sender: sender.clone(),
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
            },
            handle,
        )
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
        let cancellation_token = self.cancellation_token.clone();

        RT.spawn(async move {
            let mut just_retried = false;
            log::debug!("Fetching signature token...");
            let token = loop {
                match moosicbox_auth::fetch_signature_token(&host, &client_id, &access_token).await
                {
                    Ok(Some(token)) => break token,
                    _ => {
                        log::error!("Failed to fetch signature token");
                        sleep(Duration::from_millis(5000)).await;
                    }
                }
            };

            loop {
                let close_token = CancellationToken::new();

                if cancellation_token.is_cancelled() {
                    log::debug!("Closing tunnel");
                    break;
                }
                let (txf, rxf) = futures_channel::mpsc::unbounded();

                sender_arc.lock().unwrap().replace(txf.clone());

                log::debug!("Connecting to websocket...");
                match connect_async(format!(
                    "{}?clientId={}&sender=true&signature={token}",
                    url, client_id
                ))
                .await
                {
                    Ok((ws_stream, _)) => {
                        just_retried = false;
                        log::debug!("WebSocket handshake has been successfully completed");

                        let (write, read) = ws_stream.split();

                        let ws_writer = rxf
                            .map(|message| {
                                log::debug!(
                                    "Sending request_id={} packet_id={} size={}",
                                    message.request_id,
                                    message.packet_id,
                                    message.message.len()
                                );
                                Ok(message.message)
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
                        log::info!("Websocket connection closed");
                    }
                    Err(err) => match err {
                        Error::Http(response) => {
                            let body =
                                std::str::from_utf8(response.body().as_ref().unwrap()).unwrap();
                            log::error!("body: {}", body);
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
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage {
                    request_id,
                    packet_id,
                    message: Message::Binary(bytes.into()),
                })
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
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            sender
                .unbounded_send(TunnelResponseMessage {
                    request_id,
                    packet_id,
                    message: Message::Text(message.into()),
                })
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
        headers: HashMap<String, String>,
        reader: impl std::io::Read,
        encoding: TunnelEncoding,
    ) {
        match encoding {
            TunnelEncoding::Binary => self.send_binary(request_id, headers, reader),
            #[cfg(feature = "base64")]
            TunnelEncoding::Base64 => self.send_base64(request_id, headers, reader),
        }
    }

    async fn send_stream<E: std::error::Error + Sized>(
        &self,
        request_id: usize,
        headers: HashMap<String, String>,
        stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
        encoding: TunnelEncoding,
    ) {
        match encoding {
            TunnelEncoding::Binary => self.send_binary_stream(request_id, headers, stream).await,
            #[cfg(feature = "base64")]
            TunnelEncoding::Base64 => self.send_base64_stream(request_id, headers, stream).await,
        }
    }

    fn init_binary_request_buffer(
        request_id: usize,
        packet_id: u32,
        last: bool,
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
        headers: HashMap<String, String>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
    ) {
        let buf_size = 1024 * 32;

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;
        let mut left_over: Option<Vec<u8>> = None;
        let mut last = false;

        while !last {
            packet_id += 1;
            let mut buf = vec![0_u8; buf_size];
            let mut offset =
                Self::init_binary_request_buffer(request_id, packet_id, false, &headers, &mut buf);

            let mut left_over_size = 0_usize;
            if let Some(mut left_over_str) = left_over.take() {
                if left_over_str.len() + offset > buf.len() {
                    left_over_size = buf.len() - offset;
                    left_over.replace(left_over_str.split_off(left_over_size));
                }
                let len = left_over_str.len();
                buf[offset..offset + len].copy_from_slice(&left_over_str);
                offset += len;
                left_over_size = len;
            }

            let mut size = 0_usize;
            let mut read = 0_usize;

            if left_over.is_none() {
                let read_size = loop {
                    match stream.next().await {
                        Some(Ok(data)) => {
                            let size = data.len();
                            if offset + size <= buf_size {
                                buf[offset..offset + size].copy_from_slice(&data);
                                offset += size;
                                read += size;
                            } else {
                                buf[offset..buf_size].copy_from_slice(&data[..buf_size - offset]);
                                left_over = Some(data[buf_size - offset..].to_vec());
                                offset = buf_size;
                                break buf_size;
                            }
                        }
                        Some(Err(err)) => {
                            log::error!("Failed to read bytes: {err:?}");
                            return;
                        }
                        None => {
                            log::debug!("Received None");
                            buf[*BINARY_REQUEST_BUFFER_OFFSET - 1] = 1;
                            last = true;
                            break read;
                        }
                    }
                };

                size += read_size;
            }

            size += left_over_size;

            bytes_read += size;
            log::debug!("[{request_id}]: Read {size} bytes ({bytes_read} total) last={last}");
            let bytes = &buf[..offset];
            if let Err(err) = self.send_bytes(request_id, packet_id, bytes) {
                log::error!("Failed to send bytes: {err:?}");
                break;
            }
        }
    }

    fn send_binary(
        &self,
        request_id: usize,
        headers: HashMap<String, String>,
        mut reader: impl std::io::Read,
    ) {
        let buf_size = 1024 * 32;

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;
        let mut last = false;

        while !last {
            packet_id += 1;
            let mut buf = vec![0_u8; buf_size];
            let offset =
                Self::init_binary_request_buffer(request_id, packet_id, false, &headers, &mut buf);

            let mut read = 0;

            while offset + read < buf_size {
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
        request_id: usize,
        packet_id: u32,
        headers: &HashMap<String, String>,
        buf: &mut String,
        overflow_buf: &mut String,
    ) -> String {
        if !overflow_buf.is_empty() {
            overflow_buf.push_str(buf);
            *buf = overflow_buf.to_string();
            *overflow_buf = "".to_owned();
        }

        let mut prefix = format!("{request_id}|{packet_id}|");
        if packet_id == 1 {
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
        headers: HashMap<String, String>,
        mut reader: impl std::io::Read,
    ) {
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
                    log::debug!("Read {} bytes", bytes_read);
                    let bytes = &buf[..size];
                    let prefix = format!("{request_id}|{packet_id}|");
                    let mut base64 = general_purpose::STANDARD.encode(bytes);

                    if packet_id == 1 {
                        let mut headers_base64 = general_purpose::STANDARD
                            .encode(serde_json::to_string(&headers).unwrap().clone());
                        headers_base64.insert(0, '{');
                        headers_base64.push('}');
                        headers_base64.push_str(&base64);
                        base64 = headers_base64;
                    }

                    if !overflow_buf.is_empty() {
                        overflow_buf.push_str(&base64);
                        base64 = overflow_buf;
                        overflow_buf = "".to_owned();
                    }
                    let end = min(base64.len(), buf_size - prefix.len());
                    let data = &base64[..end];
                    overflow_buf.push_str(&base64[end..]);
                    self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                        .unwrap();

                    if size == 0 {
                        while !overflow_buf.is_empty() {
                            let base64 = overflow_buf;
                            overflow_buf = "".to_owned();
                            let end = min(base64.len(), buf_size - prefix.len());
                            let data = &base64[..end];
                            overflow_buf.push_str(&base64[end..]);
                            packet_id += 1;
                            let prefix = format!("{request_id}|{packet_id}|");
                            self.send_message(request_id, packet_id, format!("{prefix}{data}"))
                                .unwrap();
                        }

                        packet_id += 1;
                        let prefix = format!("{request_id}|{packet_id}|");
                        self.send_message(request_id, packet_id, prefix).unwrap();
                        break;
                    }
                }
                Err(_err) => break,
            }
        }
    }

    #[cfg(feature = "base64")]
    async fn send_base64_stream<E: std::error::Error + Sized>(
        &self,
        request_id: usize,
        headers: HashMap<String, String>,
        mut stream: impl Stream<Item = Result<Bytes, E>> + std::marker::Unpin,
    ) {
        use std::cmp::min;

        let buf_size = 1024 * 32;
        let mut overflow_buf = "".to_owned();

        let mut bytes_read = 0_usize;
        let mut packet_id = 0_u32;

        loop {
            packet_id += 1;

            let mut buf = "".to_owned();

            let prefix = Self::init_base64_request_buffer(
                request_id,
                packet_id,
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
                        return;
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
                .unwrap();

            if buf.is_empty() {
                let prefix = format!("{request_id}|{packet_id}|");
                self.send_message(request_id, packet_id, prefix).unwrap();
                break;
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
    ) {
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
        let client = reqwest::Client::new();

        let mut builder = match method {
            Method::Post => client.post(url),
            Method::Get => client.get(url),
            Method::Head => client.head(url),
            Method::Put => client.put(url),
            Method::Patch => client.patch(url),
            Method::Delete => client.delete(url),
        };

        builder = builder.header("user-agent", "MOOSICBOX_TUNNEL");

        if let Some(body) = payload {
            builder = builder.json(&body);
        }

        let response = builder.send().await.unwrap();
        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_str().unwrap().to_string()))
            .collect();

        self.send_stream(request_id, headers, response.bytes_stream(), encoding)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tunnel_request(
        &self,
        db: &Db,
        service_port: u16,
        request_id: usize,
        method: Method,
        path: String,
        query: Value,
        payload: Option<Value>,
        encoding: TunnelEncoding,
    ) -> Result<(), TunnelRequestError> {
        match path.to_lowercase().as_str() {
            "track" => match method {
                Method::Get => {
                    let query = serde_json::from_value::<GetTrackQuery>(query)
                        .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                    let mut headers = HashMap::new();
                    headers.insert("accept-ranges".to_string(), "bytes".to_string());

                    if let Ok(TrackSource::LocalFilePath(path)) =
                        get_track_source(query.track_id, db.clone()).await
                    {
                        static CONTENT_TYPE: &str = "content-type";
                        match query.format {
                            Some(AudioFormat::Aac) => {
                                headers.insert(CONTENT_TYPE.to_string(), "audio/mp4".to_string());
                                self.send_stream(request_id, headers,
                                    moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac_stream(
                                        path,
                                    ),
                                    encoding,
                                ).await;
                            }
                            Some(AudioFormat::Mp3) => {
                                headers.insert(CONTENT_TYPE.to_string(), "audio/mp3".to_string());
                                self.send_stream(request_id, headers,
                                    moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3_stream(
                                        path,
                                    ),
                                    encoding,
                                ).await;
                            }
                            Some(AudioFormat::Opus) => {
                                headers.insert(CONTENT_TYPE.to_string(), "audio/opus".to_string());
                                self.send_stream(request_id, headers,
                                    moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus_stream(
                                        path,
                                    ),
                                    encoding,
                                ).await;
                            }
                            _ => {
                                headers.insert(CONTENT_TYPE.to_string(), "audio/flac".to_string());
                                self.send(request_id, headers, File::open(path).unwrap(), encoding);
                            }
                        }
                    }

                    Ok(())
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            "track/info" => match method {
                Method::Get => {
                    let query = serde_json::from_value::<GetTrackInfoQuery>(query)
                        .map_err(|e| TunnelRequestError::InvalidQuery(e.to_string()))?;

                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "application/json".to_string());

                    if let Ok(track_info) = get_track_info(query.track_id, db.clone()).await {
                        let mut bytes: Vec<u8> = Vec::new();
                        serde_json::to_writer(&mut bytes, &track_info).unwrap();
                        self.send(request_id, headers, Cursor::new(bytes), encoding);
                    }

                    Ok(())
                }
                _ => Err(TunnelRequestError::UnsupportedMethod),
            },
            _ => {
                self.proxy_localhost_request(
                    service_port,
                    request_id,
                    method,
                    path,
                    query,
                    payload,
                    encoding,
                )
                .await;

                Ok(())
            }
        }
    }

    pub async fn ws_request(
        &self,
        db: &Db,
        request_id: usize,
        value: Value,
        sender: impl WebsocketSender + Send + Sync,
    ) -> Result<(), TunnelRequestError> {
        let context = WebsocketContext {
            connection_id: self.id.to_string(),
        };
        let packet_id = 1_u32;
        log::debug!("Processing tunnel ws request");
        let sender = TunnelWebsocketSender {
            id: self.id,
            packet_id,
            request_id,
            root_sender: sender,
            tunnel_sender: self.sender.lock().unwrap().clone().unwrap(),
        };
        moosicbox_ws::api::process_message(db, value, context, &sender).await?;
        Ok(())
    }
}
