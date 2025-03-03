#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, fmt::Display, task::Poll, time::SystemTime};

use bytes::Bytes;
use futures_util::{Future, Stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "base64")]
static BASE64_TUNNEL_RESPONSE_PREFIX: &str = "TUNNEL_RESPONSE:";

#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    Binary,
    #[cfg(feature = "base64")]
    Base64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TunnelWsResponse {
    pub request_id: u64,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_connection_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_connection_ids: Option<Vec<u64>>,
}

#[derive(Debug)]
pub struct TunnelResponse {
    pub request_id: u64,
    pub packet_id: u32,
    pub last: bool,
    pub bytes: Bytes,
    pub status: Option<u16>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Head,
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Head => "HEAD",
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum TunnelRequest {
    Http(TunnelHttpRequest),
    Ws(TunnelWsRequest),
    Abort(TunnelAbortRequest),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelHttpRequest {
    pub request_id: u64,
    pub method: Method,
    pub path: String,
    pub query: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Value>,
    pub encoding: TunnelEncoding,
    pub profile: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelWsRequest {
    pub conn_id: u64,
    pub request_id: u64,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<Value>,
    pub profile: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelAbortRequest {
    pub request_id: u64,
}

#[derive(Debug, Error)]
pub enum TryFromBytesError {
    #[error(transparent)]
    TryFromSlice(#[from] std::array::TryFromSliceError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl TryFrom<Bytes> for TunnelResponse {
    type Error = TryFromBytesError;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        let mut data = bytes.slice(13..);
        let request_id = u64::from_be_bytes(bytes[..8].try_into()?);
        let packet_id = u32::from_be_bytes(bytes[8..12].try_into()?);
        let last = u8::from_be_bytes(bytes[12..13].try_into()?) == 1;
        let (status, headers) = if packet_id == 1 {
            let status = u16::from_be_bytes(data[..2].try_into()?);
            data = data.slice(2..);
            let len = u32::from_be_bytes(data[..4].try_into()?) as usize;
            let headers_bytes = &data.slice(4..(4 + len));
            data = data.slice((4 + len)..);
            (Some(status), Some(serde_json::from_slice(headers_bytes)?))
        } else {
            (None, None)
        };

        Ok(Self {
            request_id,
            packet_id,
            last,
            bytes: data,
            status,
            headers,
        })
    }
}

#[cfg(feature = "base64")]
#[derive(Debug, Error)]
pub enum Base64DecodeError {
    #[error("Invalid Content: {0:?}")]
    InvalidContent(String),
    #[error(transparent)]
    Decode(#[from] base64::DecodeError),
}

#[cfg(feature = "base64")]
impl TryFrom<&str> for TunnelResponse {
    type Error = Base64DecodeError;

    fn try_from(base64: &str) -> Result<Self, Self::Error> {
        use base64::{Engine, engine::general_purpose};

        let base64 = base64
            .strip_prefix(BASE64_TUNNEL_RESPONSE_PREFIX)
            .ok_or_else(|| {
                Base64DecodeError::InvalidContent("Invalid TunnelRequest base64 data string".into())
            })?;

        let request_id_pos = base64.chars().position(|c| c == '|').ok_or_else(|| {
            Base64DecodeError::InvalidContent("Missing request_id. Expected '|' delimiter".into())
        })?;
        let request_id = base64[..request_id_pos].parse::<u64>().unwrap();

        let packet_id_pos = base64
            .chars()
            .skip(request_id_pos + 2)
            .position(|c| c == '|')
            .ok_or_else(|| {
                Base64DecodeError::InvalidContent(
                    "Missing packet_id. Expected '|' delimiter".into(),
                )
            })?;
        let packet_id = base64[request_id_pos + 1..packet_id_pos]
            .parse::<u32>()
            .unwrap();

        let last_pos = packet_id_pos + 2; // 1 (delimiter) + 1 (u8 bool byte)
        let last = base64[packet_id_pos + 1..last_pos].parse::<u8>().unwrap() == 1;

        let (status, headers) = if packet_id == 1 {
            let status_pos = last_pos + 3; // 3 digit status code
            let status = base64[last_pos..status_pos].parse::<u16>().unwrap();

            let headers_pos = base64
                .chars()
                .skip(status_pos + 2)
                .position(|c| c == '}')
                .ok_or_else(|| {
                    Base64DecodeError::InvalidContent(
                        "Missing headers. Expected '}' delimiter".into(),
                    )
                })?;

            let headers_str = &base64[status_pos + 1..headers_pos];

            (
                Some(status),
                Some(serde_json::from_str(headers_str).unwrap()),
            )
        } else {
            (None, None)
        };

        let bytes = Bytes::from(general_purpose::STANDARD.decode(base64)?);

        Ok(Self {
            request_id,
            packet_id,
            last,
            bytes,
            status,
            headers,
        })
    }
}

#[cfg(feature = "base64")]
impl TryFrom<String> for TunnelResponse {
    type Error = Base64DecodeError;

    fn try_from(base64: String) -> Result<Self, Self::Error> {
        base64.as_str().try_into()
    }
}

#[derive(Debug, Error)]
pub enum TunnelStreamError {
    #[error("TunnelStream aborted")]
    Aborted,
    #[error("TunnelStream end of stream")]
    EndOfStream,
}

pub struct TunnelStream<'a, F: Future<Output = Result<(), Box<dyn std::error::Error>>>> {
    start: SystemTime,
    request_id: u64,
    time_to_first_byte: Option<SystemTime>,
    packet_count: u32,
    byte_count: usize,
    done: bool,
    end_of_stream: bool,
    rx: UnboundedReceiver<TunnelResponse>,
    on_end: &'a dyn Fn(u64) -> F,
    packet_queue: Vec<TunnelResponse>,
    abort_token: CancellationToken,
}

impl<'a, F: Future<Output = Result<(), Box<dyn std::error::Error>>>> TunnelStream<'a, F> {
    pub fn new(
        request_id: u64,
        rx: UnboundedReceiver<TunnelResponse>,
        abort_token: CancellationToken,
        on_end: &'a impl Fn(u64) -> F,
    ) -> Self {
        Self {
            start: SystemTime::now(),
            request_id,
            time_to_first_byte: None,
            packet_count: 0,
            byte_count: 0,
            done: false,
            end_of_stream: false,
            rx,
            on_end,
            packet_queue: vec![],
            abort_token,
        }
    }

    fn process_queued_packet(
        &mut self,
    ) -> Option<std::task::Poll<Option<Result<Bytes, TunnelStreamError>>>> {
        if self
            .packet_queue
            .first()
            .is_some_and(|x| x.packet_id == self.packet_count + 1)
        {
            let response = self.packet_queue.remove(0);
            log::debug!(
                "poll_next: Sending queued packet_id={} for request_id={}",
                response.packet_id,
                self.request_id,
            );
            Some(return_polled_bytes(self, response))
        } else {
            None
        }
    }
}

fn return_polled_bytes<F: Future<Output = Result<(), Box<dyn std::error::Error>>>>(
    stream: &mut TunnelStream<F>,
    response: TunnelResponse,
) -> std::task::Poll<Option<Result<Bytes, TunnelStreamError>>> {
    if stream.time_to_first_byte.is_none() {
        stream.time_to_first_byte = Some(SystemTime::now());
    }

    stream.packet_count += 1;

    log::debug!(
        "return_polled_bytes: Received packet for request_id={} packet_count={} {} bytes last={}",
        stream.request_id,
        stream.packet_count,
        response.bytes.len(),
        response.last,
    );

    if response.last {
        stream.done = true;
    }

    stream.byte_count += response.bytes.len();

    Poll::Ready(Some(Ok(response.bytes)))
}

impl<F: Future<Output = Result<(), Box<dyn std::error::Error>>>> Stream for TunnelStream<'_, F> {
    type Item = Result<Bytes, TunnelStreamError>;

    #[allow(clippy::too_many_lines)]
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let request_id = {
            let mut stream = self.as_mut();
            let request_id = stream.request_id;

            log::trace!(
                "poll_next: TunnelStream poll for request_id={request_id} packet_count={}",
                stream.packet_count,
            );

            if stream.end_of_stream {
                log::trace!(
                    "poll_next: End of stream for request_id={request_id} packet_count={}",
                    stream.packet_count,
                );
                return stream
                    .process_queued_packet()
                    .unwrap_or(Poll::Ready(Some(Err(TunnelStreamError::EndOfStream))));
            }

            if stream.abort_token.is_cancelled() {
                log::debug!("poll_next: Stream is cancelled for request_id={request_id}",);
                return Poll::Ready(Some(Err(TunnelStreamError::Aborted)));
            }

            if stream.done {
                let end = SystemTime::now();

                log::debug!(
                    "poll_next: Byte count: {} for request_id={request_id} (received {} packet{}, took {}ms total, {}ms to first byte)",
                    stream.byte_count,
                    stream.packet_count,
                    if stream.packet_count == 1 { "" } else { "s" },
                    end.duration_since(stream.start).unwrap().as_millis(),
                    stream
                        .time_to_first_byte
                        .map(|t| t.duration_since(stream.start).unwrap().as_millis())
                        .map_or_else(|| "N/A".into(), |t| t.to_string())
                );

                (stream.on_end)(stream.request_id);

                return Poll::Ready(None);
            }

            log::debug!(
                "poll_next: Waiting for next packet for request_id={request_id} packet_count={}",
                stream.packet_count,
            );
            let response = match stream.rx.poll_recv(cx) {
                Poll::Ready(Some(response)) => response,
                Poll::Pending => {
                    log::debug!("poll_next: Pending for request_id={request_id}...");
                    return stream.process_queued_packet().unwrap_or(Poll::Pending);
                }
                Poll::Ready(None) => {
                    log::debug!("poll_next: Finished");
                    moosicbox_assert::assert!(
                        !stream.done,
                        "Stream is not finished for request_id={request_id}"
                    );
                    stream.end_of_stream = true;
                    return stream.process_queued_packet().unwrap_or(Poll::Ready(None));
                }
            };
            log::debug!(
                "poll_next: Received next packet for request_id={request_id} packet_count={}: packet_id={} status={:?} last={}",
                stream.packet_count,
                response.packet_id,
                response.status,
                response.last,
            );

            if response.packet_id == 1 && response.last {
                log::debug!(
                    "poll_next: Received first and final packet for request_id={request_id}"
                );
                return return_polled_bytes(&mut stream, response);
            }

            if response.packet_id == stream.packet_count + 1 {
                return return_polled_bytes(&mut stream, response);
            }

            log::debug!(
                "poll_next: Received future packet_id={} for request_id={request_id}. Waiting for packet {} before continuing",
                response.packet_id,
                stream.packet_count + 1,
            );

            let queued_response = if stream
                .packet_queue
                .first()
                .is_some_and(|x| x.packet_id == stream.packet_count + 1)
            {
                let response = stream.packet_queue.remove(0);
                log::debug!(
                    "poll_next: Sending queued packet_id={} for request_id={request_id}",
                    response.packet_id,
                );
                Some(return_polled_bytes(&mut stream, response))
            } else {
                None
            };

            if let Some(pos) = stream
                .packet_queue
                .iter()
                .position(|r| r.packet_id > response.packet_id)
            {
                stream.packet_queue.insert(pos, response);
            } else {
                stream.packet_queue.push(response);
            }

            if let Some(response) = queued_response {
                log::debug!("poll_next: Sending queued response for request_id={request_id}");
                return response;
            }

            request_id
        };

        log::debug!("poll_next: Re-polling for response for request_id={request_id}");
        self.poll_next(cx)
    }
}
