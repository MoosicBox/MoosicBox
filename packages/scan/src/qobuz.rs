use std::sync::Arc;

use moosicbox_core::{
    app::Db,
    sqlite::{db::DbError, models::TrackSource},
    types::AudioFormat,
};
use moosicbox_files::FetchAndSaveBytesFromRemoteUrlError;
use moosicbox_qobuz::{
    QobuzAlbum, QobuzAlbumTracksError, QobuzArtistError, QobuzFavoriteAlbumsError, QobuzTrack,
};
use thiserror::Error;
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;

use crate::{
    output::{ScanAlbum, ScanOutput, UpdateDatabaseError},
    CACHE_DIR,
};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    QobuzFavoriteAlbums(#[from] QobuzFavoriteAlbumsError),
    #[error(transparent)]
    QobuzAlbumTracks(#[from] QobuzAlbumTracksError),
    #[error(transparent)]
    QobuzArtist(#[from] QobuzArtistError),
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

pub async fn scan(db: &Db, token: CancellationToken) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let limit = 100;
    let mut offset = 0;

    while !token.is_cancelled() {
        log::debug!("Fetching Qobuz albums offset={offset} limit={limit}");

        let albums_resp =
            moosicbox_qobuz::favorite_albums(db, Some(offset), Some(limit), None, None);

        select! {
            resp = albums_resp => {
                match resp {
                    Ok((qobuz_albums, count)) => {
                        let page_count = qobuz_albums.len();

                        log::debug!("Fetched Qobuz albums offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                        scan_albums(qobuz_albums, count, db, output.clone(), token.clone()).await?;

                        if page_count < (limit as usize) {
                            break;
                        }

                        offset += page_count as u32;
                    }
                    Err(err) =>  {
                        log::warn!("Qobuz scan error: {err:?}");
                        return Err(ScanError::QobuzFavoriteAlbums(err));
                    }
                }
            },
            _ = token.cancelled() => {
                log::debug!("Cancelling Qobuz scan");
                return Ok(());
            }
        };
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished initial scan in {}ms",
        end.duration_since(start).unwrap().as_millis()
    );

    {
        let output = output.read().await;
        output.update_database(db).await?;
        output.reindex_global_search_index(db)?;
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

async fn scan_albums(
    albums: Vec<QobuzAlbum>,
    total: u32,
    db: &Db,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
) -> Result<(), ScanError> {
    log::debug!("Processing Qobuz albums count={}", albums.len());

    for album in albums {
        let count = {
            output
                .read()
                .await
                .count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1
        };

        log::info!("Scanning album {count}/{total}");

        let scan_artist = {
            output
                .write()
                .await
                .add_artist(&album.artist, &Some(album.artist_id), &None)
                .await
        };

        let scan_album = {
            scan_artist
                .write()
                .await
                .add_album(
                    &album.title,
                    &Some(album.release_date_original.clone()),
                    CACHE_DIR
                        .join(&moosicbox_files::sanitize_filename(&album.artist))
                        .join(&moosicbox_files::sanitize_filename(&album.title))
                        .to_str()
                        .unwrap(),
                    &Some(album.id.clone()),
                    &None,
                )
                .await
        };
        {
            let read_album = { scan_album.read().await.clone() };

            {
                let read_artist = { scan_artist.read().await.clone() };

                if read_artist.cover.is_none() && !read_artist.searched_cover {
                    match moosicbox_qobuz::artist(db, album.artist_id, None, None).await {
                        Ok(artist) => {
                            if let Some(url) = artist.cover_url() {
                                scan_artist.write().await.search_cover(url).await?;
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to get artist: {err:?}");
                        }
                    }
                }
            }

            if read_album.cover.is_none() && !read_album.searched_cover {
                if let Some(url) = album.cover_url() {
                    scan_album.write().await.search_cover(url).await?;
                }
            }
        }

        let limit = 100;
        let mut offset = 0;

        while !token.is_cancelled() {
            log::debug!(
                "Fetching Qobuz tracks for album album_id={} offset={offset} limit={limit}",
                album.id
            );

            let tracks_resp =
                moosicbox_qobuz::album_tracks(db, &album.id, Some(offset), Some(limit), None, None);

            select! {
                resp = tracks_resp => {
                    match resp {
                        Ok((qobuz_tracks, count)) => {
                            let page_count = qobuz_tracks.len();

                            log::debug!("Fetched Qobuz tracks offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                            scan_tracks(qobuz_tracks, scan_album.clone()).await?;

                            if page_count < (limit as usize) {
                                break;
                            }

                            offset += page_count as u32;
                        }
                        Err(err) =>  {
                            log::error!("Qobuz scan error: {err:?}");
                            break;
                        }
                    }
                },
                _ = token.cancelled() => {
                    log::debug!("Cancelling Qobuz scan");
                    return Ok(());
                }
            };
        }
    }

    Ok(())
}

async fn scan_tracks(
    tracks: Vec<QobuzTrack>,
    scan_album: Arc<RwLock<ScanAlbum>>,
) -> Result<(), ScanError> {
    log::debug!("Processing Qobuz tracks count={}", tracks.len());

    for track in tracks {
        let _ = scan_album
            .write()
            .await
            .add_track(
                &None,
                track.track_number,
                &track.title,
                track.duration as f64,
                0,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                TrackSource::Qobuz,
                &Some(track.id),
                &None,
            )
            .await;
    }

    Ok(())
}
