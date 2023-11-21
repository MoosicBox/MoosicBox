use std::{error::Error, task::Poll, time::SystemTime};

use bytes::Bytes;
use futures_util::Stream;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use tokio::sync::mpsc::Receiver;

#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    Binary,
    #[cfg(feature = "base64")]
    Base64,
}

#[derive(Debug)]
pub struct TunnelWsResponse {
    pub request_id: usize,
    pub body: Value,
}

#[derive(Debug)]
pub struct TunnelResponse {
    pub request_id: usize,
    pub packet_id: u32,
    pub bytes: Bytes,
}

impl From<Bytes> for TunnelResponse {
    fn from(bytes: Bytes) -> Self {
        let data = bytes.slice(12..);
        let request_id = usize::from_be_bytes(bytes[..8].try_into().unwrap());
        let packet_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());

        TunnelResponse {
            request_id,
            packet_id,
            bytes: data,
        }
    }
}

pub struct TunnelStream<'a> {
    start: SystemTime,
    request_id: usize,
    time_to_first_byte: Option<SystemTime>,
    packet_count: u32,
    byte_count: usize,
    rx: Receiver<TunnelResponse>,
    on_end: &'a dyn Fn(usize),
    packet_queue: Vec<TunnelResponse>,
}

impl<'a> TunnelStream<'a> {
    pub fn new(
        request_id: usize,
        rx: Receiver<TunnelResponse>,
        on_end: &'a impl Fn(usize),
    ) -> TunnelStream<'a> {
        TunnelStream {
            start: SystemTime::now(),
            request_id,
            time_to_first_byte: None,
            packet_count: 0,
            byte_count: 0,
            rx,
            on_end,
            packet_queue: vec![],
        }
    }
}

fn return_polled_bytes(
    stream: &mut TunnelStream,
    response: TunnelResponse,
) -> std::task::Poll<Option<Result<Bytes, Box<dyn Error>>>> {
    if stream.time_to_first_byte.is_none() {
        stream.time_to_first_byte = Some(SystemTime::now());
    }
    stream.packet_count += 1;
    debug!(
        "Received packet for {} {} {} bytes",
        stream.request_id,
        stream.packet_count,
        response.bytes.len()
    );

    if response.bytes.is_empty() {
        let end = SystemTime::now();

        debug!(
            "Byte count: {} (received {} packet{}, took {}ms total, {}ms to first byte)",
            stream.byte_count,
            stream.packet_count,
            if stream.packet_count == 1 { "" } else { "s" },
            end.duration_since(stream.start).unwrap().as_millis(),
            stream
                .time_to_first_byte
                .map(|t| t.duration_since(stream.start).unwrap().as_millis())
                .map(|t| t.to_string())
                .unwrap_or("N/A".into())
        );

        (stream.on_end)(stream.request_id);

        return Poll::Ready(None);
    }

    stream.byte_count += response.bytes.len();

    Poll::Ready(Some(Ok(response.bytes)))
}

impl Stream for TunnelStream<'_> {
    type Item = Result<Bytes, Box<dyn Error>>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        debug!("Waiting for next packet");
        let response = match stream.rx.poll_recv(cx) {
            Poll::Ready(Some(response)) => response,
            Poll::Pending => {
                debug!("Pending...");
                return Poll::Pending;
            }
            Poll::Ready(None) => {
                debug!("Finished");
                return Poll::Ready(None);
            }
        };

        if stream
            .packet_queue
            .first()
            .map(|n| n.packet_id == stream.packet_count + 1)
            .is_some_and(|n| n)
        {
            let response = stream.packet_queue.remove(0);
            debug!("Sending queued packet {}", response.packet_id);
            return return_polled_bytes(stream, response);
        }

        if response.packet_id > stream.packet_count + 1 {
            debug!(
                "Received future packet {}. Waiting for packet {} before continuing",
                response.packet_id,
                stream.packet_count + 1
            );
            if let Some(pos) = stream
                .packet_queue
                .iter()
                .position(|r| r.packet_id > response.packet_id)
            {
                stream.packet_queue.insert(pos, response);
            } else {
                stream.packet_queue.push(response);
            }
            return Poll::Pending;
        }

        return_polled_bytes(stream, response)
    }
}
