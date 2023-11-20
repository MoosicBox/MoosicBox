use std::{
    collections::HashMap,
    sync::Mutex,
    task::Poll,
    time::{Duration, SystemTime},
};

use bytes::Bytes;
use crossbeam_channel::{Receiver, RecvTimeoutError};
use futures_util::Stream;
use log::{debug, error};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    Binary,
    #[cfg(feature = "base64")]
    Base64,
}

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

pub struct TunnelStream {
    start: SystemTime,
    request_id: usize,
    time_to_first_byte: Option<SystemTime>,
    packet_count: u32,
    byte_count: usize,
    rx: Receiver<TunnelResponse>,
}

impl TunnelStream {
    pub fn new(request_id: usize, rx: Receiver<TunnelResponse>) -> TunnelStream {
        TunnelStream {
            start: SystemTime::now(),
            request_id,
            time_to_first_byte: None,
            packet_count: 0,
            byte_count: 0,
            rx,
        }
    }
}

static PACKET_QUEUE: Lazy<Mutex<HashMap<usize, Vec<TunnelResponse>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn return_polled_bytes(
    stream: &mut TunnelStream,
    response: TunnelResponse,
) -> std::task::Poll<Option<Result<Bytes, RecvTimeoutError>>> {
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

        PACKET_QUEUE.lock().unwrap().remove(&response.request_id);

        return Poll::Ready(None);
    }

    stream.byte_count += response.bytes.len();

    Poll::Ready(Some(Ok(response.bytes)))
}

impl Stream for TunnelStream {
    type Item = Result<Bytes, RecvTimeoutError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        debug!("Waiting for next packet");
        let response = match stream.rx.recv_timeout(Duration::from_secs(10)) {
            Ok(response) => response,
            Err(err) => {
                error!(
                    "Timed out waiting for next packet for request {}, packet {}",
                    stream.request_id,
                    stream.packet_count + 1
                );
                return Poll::Ready(Some(Err(err)));
            }
        };

        if let Some(queue) = PACKET_QUEUE.lock().unwrap().get_mut(&response.request_id) {
            if queue
                .iter()
                .next()
                .map(|n| n.packet_id == stream.packet_count + 1)
                .is_some_and(|n| n)
            {
                return return_polled_bytes(stream, queue.remove(0));
            }
        }

        if response.packet_id > stream.packet_count + 1 {
            let mut queues = PACKET_QUEUE.lock().unwrap();
            if let Some(queue) = queues.get_mut(&response.request_id) {
                if let Some(pos) = queue.iter().position(|r| r.packet_id > response.packet_id) {
                    queue.insert(pos, response);
                } else {
                    queue.push(response);
                }
            } else {
                queues.insert(response.request_id, vec![response]);
            }
            return Poll::Pending;
        }

        return_polled_bytes(stream, response)
    }
}
