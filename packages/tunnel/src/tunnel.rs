use std::{fs::File, io::Read};

use moosicbox_core::app::Db;
use moosicbox_files::{
    api::GetTrackQuery,
    files::track::{get_track_source, TrackSource},
};
use serde_json::Value;
use thiserror::Error;

use crate::ws::sender::send_bytes;

#[derive(Debug, Error)]
pub enum TunnelRequestError {
    #[error("Invalid Payload: '{0}' ({1})")]
    InvalidPayload(String, String),
}

pub enum TunnelResponse {
    Stream,
    Empty,
}

pub async fn tunnel_request(
    db: &Db,
    id: usize,
    path: String,
    payload: Value,
) -> Result<TunnelResponse, TunnelRequestError> {
    let buf_size = 1024 * 32;
    match path.as_str() {
        "track" => {
            let query = serde_json::from_value::<GetTrackQuery>(payload.clone()).map_err(|e| {
                TunnelRequestError::InvalidPayload(payload.to_string(), e.to_string())
            })?;

            if let Ok(TrackSource::LocalFilePath(path)) =
                get_track_source(query.track_id, db.clone()).await
            {
                let mut file = File::open(path).unwrap();
                let id_bytes = id.to_be_bytes();

                loop {
                    let mut buf = vec![0_u8; buf_size];
                    buf[..8].copy_from_slice(&id_bytes);
                    match file.read(&mut buf[8..]) {
                        Ok(size) => {
                            send_bytes(&buf[..(size + 8)]).unwrap();
                            if size == 0 {
                                break;
                            }
                        }
                        Err(_err) => break,
                    }
                }
            }
            Ok(TunnelResponse::Stream)
        }
        "albums" => Ok(TunnelResponse::Empty),
        _ => Ok(TunnelResponse::Empty),
    }
}
