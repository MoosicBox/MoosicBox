use std::{
    cmp::min,
    fs::File,
    io::Read,
    task::Poll,
    thread,
    time::{Duration, SystemTime},
};

use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use crossbeam_channel::{Receiver, RecvTimeoutError};
use futures_util::Stream;
use log::debug;
use moosicbox_core::app::Db;
use moosicbox_files::files::track::{get_track_info, get_track_source, TrackSource};
use serde::{Deserialize, Serialize};
use serde_aux::prelude::*;
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;

use crate::ws::sender::{send_bytes, send_message, TunnelResponse};

#[derive(Debug, Error)]
pub enum TunnelRequestError {
    #[error("Invalid Query: '{0}' ({1})")]
    InvalidQuery(String, String),
}

#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    Binary,
    Base64,
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

pub async fn tunnel_request(
    db: &Db,
    id: usize,
    method: Method,
    path: String,
    query: Value,
    _payload: Value,
    encoding: TunnelEncoding,
) -> Result<(), TunnelRequestError> {
    let buf_size = 1024 * 32;
    let mut overflow_buf = "".to_owned();

    match path.as_str() {
        "track" => match method {
            Method::Get => {
                let query =
                    serde_json::from_value::<GetTrackQuery>(query.clone()).map_err(|e| {
                        TunnelRequestError::InvalidQuery(query.to_string(), e.to_string())
                    })?;

                if let Ok(TrackSource::LocalFilePath(path)) =
                    get_track_source(query.track_id, db.clone()).await
                {
                    let mut file = File::open(path).unwrap();
                    let mut bytes_read = 0_usize;
                    let mut packet_id = 0_u32;

                    loop {
                        let mut buf = vec![0_u8; buf_size];
                        let offset = if encoding == TunnelEncoding::Binary {
                            let id_bytes = id.to_be_bytes();
                            let len = id_bytes.len();
                            buf[..len].copy_from_slice(&id_bytes);
                            len
                        } else {
                            0_usize
                        };
                        let offset = if encoding == TunnelEncoding::Binary {
                            let packet_id_bytes = packet_id.to_be_bytes();
                            let len = packet_id_bytes.len();
                            buf[offset..(offset + len)].copy_from_slice(&packet_id_bytes);
                            offset + len
                        } else {
                            offset
                        };
                        match file.read(&mut buf[offset..]) {
                            Ok(size) => {
                                packet_id += 1;
                                bytes_read += size;
                                debug!("Read {} bytes", bytes_read);
                                let bytes = &buf[..(size + offset)];
                                match encoding {
                                    TunnelEncoding::Base64 => {
                                        let prefix = format!("{id}|{packet_id}|");
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
                                        thread::sleep(std::time::Duration::from_millis(2000));
                                        send_message(format!("{prefix}{data}")).unwrap();

                                        if size == 0 {
                                            while !overflow_buf.is_empty() {
                                                let base64 = overflow_buf;
                                                overflow_buf = "".to_owned();
                                                let end =
                                                    min(base64.len(), buf_size - prefix.len());
                                                let data = &base64[..end];
                                                overflow_buf.push_str(&base64[end..]);
                                                packet_id += 1;
                                                let prefix = format!("{id}|{packet_id}|");
                                                send_message(format!("{prefix}{data}")).unwrap();
                                            }

                                            packet_id += 1;
                                            let prefix = format!("{id}|{packet_id}|");
                                            send_message(prefix).unwrap();
                                        }
                                    }
                                    TunnelEncoding::Binary => {
                                        send_bytes(bytes).unwrap();
                                    }
                                }
                                if size == 0 {
                                    break;
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                }
                Ok(())
            }
            _ => todo!(),
        },
        "track/info" => match method {
            Method::Get => {
                let mut packet_id = 0_u32;
                let query =
                    serde_json::from_value::<GetTrackInfoQuery>(query.clone()).map_err(|e| {
                        TunnelRequestError::InvalidQuery(query.to_string(), e.to_string())
                    })?;

                if let Ok(track_info) = get_track_info(query.track_id, db.clone()).await {
                    packet_id += 1;
                    let mut buf = vec![0_u8; 12];
                    let request_id_bytes = id.to_be_bytes();
                    let packet_id_bytes = packet_id.to_be_bytes();
                    buf[..8].copy_from_slice(&request_id_bytes);
                    buf[8..12].copy_from_slice(&packet_id_bytes);
                    let mut bytes: Vec<u8> = Vec::new();
                    serde_json::to_writer(&mut bytes, &track_info).unwrap();
                    buf.extend_from_slice(&bytes);

                    match encoding {
                        TunnelEncoding::Base64 => {
                            let prefix = format!("{id}|{packet_id}|");
                            let mut base64 = general_purpose::STANDARD.encode(bytes);
                            base64.insert(0, '{');
                            base64.push('}');
                            send_message(format!("{prefix}{base64}")).unwrap();
                            thread::sleep(std::time::Duration::from_millis(1000));

                            packet_id += 1;
                            let prefix = format!("{id}|{packet_id}|");
                            send_message(prefix).unwrap();
                        }
                        TunnelEncoding::Binary => {
                            let mut prefix_bytes = [0; 12];
                            prefix_bytes.copy_from_slice(&buf[..12]);
                            send_bytes(buf).unwrap();
                            send_bytes(prefix_bytes).unwrap();
                        }
                    }
                }

                Ok(())
            }
            _ => todo!(),
        },
        "albums" => Ok(()),
        _ => Ok(()),
    }
}

pub struct TunnelStream {
    start: SystemTime,
    request_id: usize,
    time_to_first_byte: Option<SystemTime>,
    packet_count: i32,
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

impl Stream for TunnelStream {
    type Item = Result<Bytes, RecvTimeoutError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        debug!("Waiting for next packet");
        let response = stream.rx.recv_timeout(Duration::from_secs(10))?;

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

            return Poll::Ready(None);
        }

        stream.byte_count += response.bytes.len();

        Poll::Ready(Some(Ok(response.bytes)))
    }
}
