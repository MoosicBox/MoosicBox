#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

use futures::StreamExt;
use moosicbox_core::{app::Db, types::AudioFormat};
use moosicbox_files::files::track::{
    get_track_bytes, get_track_source, GetTrackBytesError, TrackAudioQuality, TrackSourceError,
};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    TrackSource(#[from] TrackSourceError),
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub async fn download(
    db: &Db,
    track_id: u64,
    quality: Option<TrackAudioQuality>,
) -> Result<(), DownloadError> {
    let source = get_track_source(track_id as i32, db, quality).await?;
    let mut bytes = get_track_bytes(db, track_id, source, AudioFormat::Source, false).await?;

    let mut reader = bytes.stream.as_mut();
    let mut writer = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("test.flac")
        .await?;

    let mut total = 0_usize;

    while let Some(Ok(data)) = reader.next().await {
        total += data.len();
        log::debug!("Writing bytes {} ({total} total)", data.len());
        writer.write(&data).await?;
    }

    Ok(())
}
