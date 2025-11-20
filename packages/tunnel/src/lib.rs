//! Tunneling protocol for HTTP and WebSocket requests over persistent connections.
//!
//! This crate provides types and utilities for tunneling HTTP and WebSocket requests through
//! a persistent connection, enabling bidirectional communication between clients and servers.
//! The tunnel protocol supports request streaming, packet ordering, and multiple encoding formats.
//!
//! # Main Components
//!
//! * [`TunnelRequest`] - Tagged enum for HTTP, WebSocket, or abort requests
//! * [`TunnelHttpRequest`] - HTTP request metadata and payload
//! * [`TunnelWsRequest`] - WebSocket request metadata and payload
//! * [`TunnelResponse`] - Response packets with headers, status, and body bytes
//! * [`TunnelStream`] - Async stream of response packets for a request
//! * [`TunnelEncoding`] - Encoding format for response data (binary or base64)
//!
//! # Features
//!
//! * `base64` (default) - Enables base64 encoding support for text-safe transmission

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, task::Poll, time::SystemTime};

use bytes::Bytes;
use futures_util::{Future, Stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use switchy_async::util::CancellationToken;
use switchy_http::models::Method;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;

/// Prefix used to identify base64-encoded tunnel response strings.
#[cfg(feature = "base64")]
static BASE64_TUNNEL_RESPONSE_PREFIX: &str = "TUNNEL_RESPONSE:";

/// Encoding format for tunnel response data.
#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    /// Binary encoding for raw bytes.
    Binary,
    /// Base64 encoding for text-safe transmission.
    #[cfg(feature = "base64")]
    Base64,
}

/// Response for a WebSocket tunnel request.
#[derive(Debug, Serialize, Deserialize)]
pub struct TunnelWsResponse {
    /// Unique identifier for the request.
    pub request_id: u64,
    /// Response body payload.
    pub body: Value,
    /// Connection IDs to exclude from receiving this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_connection_ids: Option<Vec<u64>>,
    /// Connection IDs to send this response to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_connection_ids: Option<Vec<u64>>,
}

/// Response packet from a tunnel HTTP request.
#[derive(Debug)]
pub struct TunnelResponse {
    /// Unique identifier for the request.
    pub request_id: u64,
    /// Packet sequence number (1-indexed).
    pub packet_id: u32,
    /// Whether this is the final packet for this request.
    pub last: bool,
    /// Response body bytes.
    pub bytes: Bytes,
    /// HTTP status code (present in first packet only).
    pub status: Option<u16>,
    /// HTTP headers (present in first packet only).
    pub headers: Option<BTreeMap<String, String>>,
}

/// Request sent through the tunnel.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum TunnelRequest {
    /// HTTP request.
    Http(TunnelHttpRequest),
    /// WebSocket request.
    Ws(TunnelWsRequest),
    /// Request to abort an in-progress request.
    Abort(TunnelAbortRequest),
}

/// HTTP request sent through the tunnel.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelHttpRequest {
    /// Unique identifier for the request.
    pub request_id: u64,
    /// HTTP method.
    pub method: Method,
    /// Request path.
    pub path: String,
    /// Query parameters.
    pub query: Value,
    /// Request body payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// HTTP headers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Value>,
    /// Encoding format for the response.
    pub encoding: TunnelEncoding,
    /// Profile identifier for the request.
    pub profile: Option<String>,
}

/// WebSocket request sent through the tunnel.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelWsRequest {
    /// WebSocket connection identifier.
    pub conn_id: u64,
    /// Unique identifier for the request.
    pub request_id: u64,
    /// Request body payload.
    pub body: Value,
    /// Connection identifier from the original request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<Value>,
    /// Profile identifier for the request.
    pub profile: Option<String>,
}

/// Request to abort an in-progress tunnel request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelAbortRequest {
    /// Unique identifier for the request to abort.
    pub request_id: u64,
}

/// Errors that can occur when converting bytes to a tunnel response.
#[derive(Debug, Error)]
pub enum TryFromBytesError {
    /// Failed to convert byte slice to array.
    #[error(transparent)]
    TryFromSlice(#[from] std::array::TryFromSliceError),
    /// Failed to deserialize JSON data.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl TryFrom<Bytes> for TunnelResponse {
    type Error = TryFromBytesError;

    /// Converts binary bytes to a tunnel response.
    ///
    /// # Errors
    ///
    /// * Returns [`TryFromBytesError::TryFromSlice`] if byte conversion fails
    /// * Returns [`TryFromBytesError::Serde`] if JSON deserialization fails
    ///
    /// # Panics
    ///
    /// Panics if the byte slice is shorter than 13 bytes (minimum required for header data).
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

/// Errors that can occur when decoding base64-encoded tunnel responses.
#[cfg(feature = "base64")]
#[derive(Debug, Error)]
pub enum Base64DecodeError {
    /// Invalid content format.
    #[error("Invalid Content: {0:?}")]
    InvalidContent(String),
    /// Failed to decode base64 data.
    #[error(transparent)]
    Decode(#[from] base64::DecodeError),
}

#[cfg(feature = "base64")]
impl TryFrom<&str> for TunnelResponse {
    type Error = Base64DecodeError;

    /// Converts a base64-encoded string to a tunnel response.
    ///
    /// # Errors
    ///
    /// * Returns [`Base64DecodeError::InvalidContent`] if the string format is invalid
    /// * Returns [`Base64DecodeError::Decode`] if base64 decoding fails
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// * Parsing `request_id`, `packet_id`, last flag, or status code from the string fails
    /// * JSON deserialization of headers fails
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

    /// Converts a base64-encoded string to a tunnel response.
    ///
    /// # Errors
    ///
    /// * Returns [`Base64DecodeError::InvalidContent`] if the string format is invalid
    /// * Returns [`Base64DecodeError::Decode`] if base64 decoding fails
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// * Parsing `request_id`, `packet_id`, last flag, or status code from the string fails
    /// * JSON deserialization of headers fails
    fn try_from(base64: String) -> Result<Self, Self::Error> {
        base64.as_str().try_into()
    }
}

/// Errors that can occur when streaming tunnel responses.
#[derive(Debug, Error)]
pub enum TunnelStreamError {
    /// Stream was aborted before completion.
    #[error("TunnelStream aborted")]
    Aborted,
    /// Stream reached end without completing.
    #[error("TunnelStream end of stream")]
    EndOfStream,
}

/// Stream of tunnel response packets.
///
/// Implements [`Stream`] to provide ordered response packets for a tunnel request.
/// Handles out-of-order packet delivery and tracks performance metrics.
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
    /// Creates a new tunnel stream.
    ///
    /// # Arguments
    ///
    /// * `request_id` - Unique identifier for the request
    /// * `rx` - Channel receiver for incoming response packets
    /// * `abort_token` - Token to signal stream cancellation
    /// * `on_end` - Callback invoked when the stream completes
    #[must_use]
    pub fn new(
        request_id: u64,
        rx: UnboundedReceiver<TunnelResponse>,
        abort_token: CancellationToken,
        on_end: &'a impl Fn(u64) -> F,
    ) -> Self {
        Self {
            start: switchy_time::now(),
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

    /// Processes the next queued packet if available and in sequence.
    ///
    /// Returns `Some(Poll)` if a packet was processed, `None` if no packet is ready.
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

/// Converts a tunnel response into a polled stream item.
///
/// Updates stream metrics including time to first byte, packet count, and byte count.
/// Marks the stream as done if the response is the final packet.
fn return_polled_bytes<F: Future<Output = Result<(), Box<dyn std::error::Error>>>>(
    stream: &mut TunnelStream<F>,
    response: TunnelResponse,
) -> std::task::Poll<Option<Result<Bytes, TunnelStreamError>>> {
    if stream.time_to_first_byte.is_none() {
        stream.time_to_first_byte = Some(switchy_time::now());
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
                let end = switchy_time::now();

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Helper function to create test binary bytes for `TunnelResponse`
    fn create_binary_response(
        request_id: u64,
        packet_id: u32,
        last: bool,
        status: Option<u16>,
        headers: Option<BTreeMap<String, String>>,
        body: &[u8],
    ) -> Bytes {
        let mut data = Vec::new();

        // Request ID (8 bytes)
        data.extend_from_slice(&request_id.to_be_bytes());

        // Packet ID (4 bytes)
        data.extend_from_slice(&packet_id.to_be_bytes());

        // Last flag (1 byte)
        data.push(u8::from(last));

        // If first packet, add status and headers
        if packet_id == 1 {
            let status = status.expect("First packet must have status");
            data.extend_from_slice(&status.to_be_bytes());

            let headers = headers.expect("First packet must have headers");
            let headers_json = serde_json::to_vec(&headers).unwrap();
            let headers_len = u32::try_from(headers_json.len()).unwrap();
            data.extend_from_slice(&headers_len.to_be_bytes());
            data.extend_from_slice(&headers_json);
        }

        // Body
        data.extend_from_slice(body);

        Bytes::from(data)
    }

    #[test]
    fn test_tunnel_response_from_bytes_first_packet() {
        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-custom".to_string(), "test-value".to_string());

        let body = b"test response body";
        let bytes = create_binary_response(12345, 1, false, Some(200), Some(headers.clone()), body);

        let response = TunnelResponse::try_from(bytes).unwrap();

        assert_eq!(response.request_id, 12345);
        assert_eq!(response.packet_id, 1);
        assert!(!response.last);
        assert_eq!(response.status, Some(200));
        assert_eq!(response.headers, Some(headers));
        assert_eq!(response.bytes.as_ref(), body);
    }

    #[test]
    fn test_tunnel_response_from_bytes_subsequent_packet() {
        let body = b"more data";
        let bytes = create_binary_response(12345, 2, false, None, None, body);

        let response = TunnelResponse::try_from(bytes).unwrap();

        assert_eq!(response.request_id, 12345);
        assert_eq!(response.packet_id, 2);
        assert!(!response.last);
        assert_eq!(response.status, None);
        assert_eq!(response.headers, None);
        assert_eq!(response.bytes.as_ref(), body);
    }

    #[test]
    fn test_tunnel_response_from_bytes_final_packet() {
        let body = b"final chunk";
        let bytes = create_binary_response(12345, 3, true, None, None, body);

        let response = TunnelResponse::try_from(bytes).unwrap();

        assert_eq!(response.request_id, 12345);
        assert_eq!(response.packet_id, 3);
        assert!(response.last);
        assert_eq!(response.status, None);
        assert_eq!(response.headers, None);
        assert_eq!(response.bytes.as_ref(), body);
    }

    #[test]
    fn test_tunnel_response_from_bytes_empty_body() {
        let headers = BTreeMap::new();
        let bytes = create_binary_response(999, 1, true, Some(204), Some(headers.clone()), &[]);

        let response = TunnelResponse::try_from(bytes).unwrap();

        assert_eq!(response.request_id, 999);
        assert_eq!(response.packet_id, 1);
        assert!(response.last);
        assert_eq!(response.status, Some(204));
        assert_eq!(response.headers, Some(headers));
        assert!(response.bytes.is_empty());
    }

    #[test]
    fn test_tunnel_response_from_bytes_large_headers() {
        let mut headers = BTreeMap::new();
        for i in 0..50 {
            headers.insert(format!("header-{i}"), format!("value-{i}"));
        }

        let body = b"body";
        let bytes = create_binary_response(7777, 1, false, Some(200), Some(headers.clone()), body);

        let response = TunnelResponse::try_from(bytes).unwrap();

        assert_eq!(response.request_id, 7777);
        assert_eq!(response.headers, Some(headers));
        assert_eq!(response.bytes.as_ref(), body);
    }

    #[test]
    #[should_panic(expected = "range start must not be greater than end")]
    fn test_tunnel_response_from_bytes_too_short() {
        // Less than 13 bytes minimum
        let bytes = Bytes::from(vec![1, 2, 3, 4, 5]);
        let _response = TunnelResponse::try_from(bytes).unwrap();
    }

    #[test]
    fn test_tunnel_response_from_bytes_error_invalid_json_headers() {
        let mut data = Vec::new();
        data.extend_from_slice(&123_u64.to_be_bytes()); // request_id
        data.extend_from_slice(&1_u32.to_be_bytes()); // packet_id
        data.push(0); // last = false
        data.extend_from_slice(&200_u16.to_be_bytes()); // status
        data.extend_from_slice(&5_u32.to_be_bytes()); // headers length
        data.extend_from_slice(b"{bad}"); // invalid JSON

        let bytes = Bytes::from(data);
        let result = TunnelResponse::try_from(bytes);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TryFromBytesError::Serde(_)));
    }

    #[cfg(feature = "base64")]
    #[test]
    fn test_tunnel_response_from_base64_missing_prefix() {
        let result = TunnelResponse::try_from("12345|1|0200{}|dGVzdA==");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Base64DecodeError::InvalidContent(_)
        ));
    }

    #[cfg(feature = "base64")]
    #[test]
    fn test_tunnel_response_from_base64_missing_request_id_delimiter() {
        let invalid = format!("{BASE64_TUNNEL_RESPONSE_PREFIX}12345");
        let result = TunnelResponse::try_from(invalid.as_str());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Base64DecodeError::InvalidContent(_)
        ));
    }

    #[cfg(feature = "base64")]
    #[test]
    fn test_tunnel_response_from_base64_missing_packet_id_delimiter() {
        let invalid = format!("{BASE64_TUNNEL_RESPONSE_PREFIX}12345|1");
        let result = TunnelResponse::try_from(invalid.as_str());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Base64DecodeError::InvalidContent(_)
        ));
    }

    #[test]
    fn test_tunnel_request_http_serialization() {
        let request = TunnelRequest::Http(TunnelHttpRequest {
            request_id: 123,
            method: Method::Get,
            path: "/api/test".to_string(),
            query: serde_json::json!({"foo": "bar"}),
            payload: Some(serde_json::json!({"data": "value"})),
            headers: Some(serde_json::json!({"Authorization": "Bearer token"})),
            encoding: TunnelEncoding::Binary,
            profile: Some("test-profile".to_string()),
        });

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: TunnelRequest = serde_json::from_str(&json).unwrap();

        match deserialized {
            TunnelRequest::Http(req) => {
                assert_eq!(req.request_id, 123);
                assert_eq!(req.method, Method::Get);
                assert_eq!(req.path, "/api/test");
                assert_eq!(req.encoding, TunnelEncoding::Binary);
            }
            _ => panic!("Expected HTTP request"),
        }
    }

    #[test]
    fn test_tunnel_request_ws_serialization() {
        let request = TunnelRequest::Ws(TunnelWsRequest {
            conn_id: 456,
            request_id: 789,
            body: serde_json::json!({"message": "hello"}),
            connection_id: Some(serde_json::json!(42)),
            profile: None,
        });

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: TunnelRequest = serde_json::from_str(&json).unwrap();

        match deserialized {
            TunnelRequest::Ws(req) => {
                assert_eq!(req.conn_id, 456);
                assert_eq!(req.request_id, 789);
                assert_eq!(req.body, serde_json::json!({"message": "hello"}));
            }
            _ => panic!("Expected WS request"),
        }
    }

    #[test]
    fn test_tunnel_request_abort_serialization() {
        let request = TunnelRequest::Abort(TunnelAbortRequest { request_id: 999 });

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: TunnelRequest = serde_json::from_str(&json).unwrap();

        match deserialized {
            TunnelRequest::Abort(req) => {
                assert_eq!(req.request_id, 999);
            }
            _ => panic!("Expected Abort request"),
        }
    }

    #[test]
    fn test_tunnel_ws_response_serialization() {
        let response = TunnelWsResponse {
            request_id: 123,
            body: serde_json::json!({"status": "ok"}),
            exclude_connection_ids: Some(vec![1, 2, 3]),
            to_connection_ids: Some(vec![4, 5, 6]),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: TunnelWsResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.request_id, 123);
        assert_eq!(deserialized.exclude_connection_ids, Some(vec![1, 2, 3]));
        assert_eq!(deserialized.to_connection_ids, Some(vec![4, 5, 6]));
    }

    #[test]
    fn test_tunnel_ws_response_optional_fields_omitted() {
        let response = TunnelWsResponse {
            request_id: 456,
            body: serde_json::json!({"data": "test"}),
            exclude_connection_ids: None,
            to_connection_ids: None,
        };

        let json = serde_json::to_string(&response).unwrap();

        // Verify that None fields are not serialized
        assert!(!json.contains("exclude_connection_ids"));
        assert!(!json.contains("to_connection_ids"));

        let deserialized: TunnelWsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.request_id, 456);
        assert_eq!(deserialized.exclude_connection_ids, None);
        assert_eq!(deserialized.to_connection_ids, None);
    }

    #[test]
    fn test_tunnel_encoding_serialization() {
        let binary = TunnelEncoding::Binary;
        let json = serde_json::to_string(&binary).unwrap();
        assert_eq!(json, "\"BINARY\"");

        let deserialized: TunnelEncoding = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TunnelEncoding::Binary);
    }

    #[cfg(feature = "base64")]
    #[test]
    fn test_tunnel_encoding_base64_serialization() {
        let base64 = TunnelEncoding::Base64;
        let json = serde_json::to_string(&base64).unwrap();
        assert_eq!(json, "\"BASE64\"");

        let deserialized: TunnelEncoding = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TunnelEncoding::Base64);
    }

    #[test]
    fn test_tunnel_http_request_optional_fields() {
        let request = TunnelHttpRequest {
            request_id: 1,
            method: Method::Post,
            path: "/test".to_string(),
            query: serde_json::json!({}),
            payload: None,
            headers: None,
            encoding: TunnelEncoding::Binary,
            profile: None,
        };

        let json = serde_json::to_string(&request).unwrap();

        // Verify optional fields are omitted when None
        assert!(!json.contains("payload"));
        assert!(!json.contains("headers"));

        let deserialized: TunnelHttpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.payload, None);
        assert_eq!(deserialized.headers, None);
        assert_eq!(deserialized.profile, None);
    }

    #[test]
    fn test_tunnel_request_tagged_enum_format() {
        let http_request = TunnelRequest::Http(TunnelHttpRequest {
            request_id: 1,
            method: Method::Get,
            path: "/".to_string(),
            query: serde_json::json!({}),
            payload: None,
            headers: None,
            encoding: TunnelEncoding::Binary,
            profile: None,
        });

        let json = serde_json::to_string(&http_request).unwrap();

        // Verify SCREAMING_SNAKE_CASE and tag format
        assert!(json.contains("\"type\":\"HTTP\""));

        let ws_request = TunnelRequest::Ws(TunnelWsRequest {
            conn_id: 1,
            request_id: 2,
            body: serde_json::json!({}),
            connection_id: None,
            profile: None,
        });

        let json = serde_json::to_string(&ws_request).unwrap();
        assert!(json.contains("\"type\":\"WS\""));

        let abort_request = TunnelRequest::Abort(TunnelAbortRequest { request_id: 3 });
        let json = serde_json::to_string(&abort_request).unwrap();
        assert!(json.contains("\"type\":\"ABORT\""));
    }
}
