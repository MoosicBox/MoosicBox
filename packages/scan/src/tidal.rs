use std::sync::Arc;

use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{
            tidal::{TidalAlbum, TidalAlbumImageSize, TidalArtistImageSize, TidalTrack},
            TrackApiSource,
        },
    },
    types::AudioFormat,
};
use moosicbox_database::Database;
use moosicbox_files::FetchAndSaveBytesFromRemoteUrlError;
use moosicbox_tidal::{TidalAlbumTracksError, TidalArtistError, TidalFavoriteAlbumsError};
use thiserror::Error;
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;

use crate::output::{ScanAlbum, ScanOutput, UpdateDatabaseError};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TidalFavoriteAlbums(#[from] TidalFavoriteAlbumsError),
    #[error(transparent)]
    TidalAlbumTracks(#[from] TidalAlbumTracksError),
    #[error(transparent)]
    TidalArtist(#[from] TidalArtistError),
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

pub async fn scan(db: Arc<Box<dyn Database>>, token: CancellationToken) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let limit = 100;
    let mut offset = 0;

    while !token.is_cancelled() {
        log::debug!("Fetching Tidal albums offset={offset} limit={limit}");

        let albums_resp = moosicbox_tidal::favorite_albums(
            db.clone(),
            Some(offset),
            Some(limit),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        select! {
            resp = albums_resp => {
                match resp {
                    Ok(page) => {
                        let page_count = page.len();

                        log::debug!("Fetched Tidal albums offset={offset} limit={limit}: page_count={page_count}, total_count={}", page.total().unwrap());

                        scan_albums(&page, page.total().unwrap(), db.clone(), output.clone(), Some(token.clone())).await?;

                        if page_count < (limit as usize) {
                            break;
                        }

                        offset += page_count as u32;
                    }
                    Err(err) =>  {
                        log::warn!("Tidal scan error: {err:?}");
                        return Err(ScanError::TidalFavoriteAlbums(err));
                    }
                }
            },
            _ = token.cancelled() => {
                log::debug!("Cancelling Tidal scan");
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
        output.update_database(db.clone()).await?;
        output.reindex_global_search_index(&db).await?;
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

pub async fn scan_albums(
    albums: &[TidalAlbum],
    total: u32,
    db: Arc<Box<dyn Database>>,
    output: Arc<RwLock<ScanOutput>>,
    token: Option<CancellationToken>,
) -> Result<(), ScanError> {
    log::debug!("Processing Tidal albums count={}", albums.len());

    let token = token.unwrap_or_default();

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
                .add_artist(&album.artist, &None, &Some(album.artist_id))
                .await
        };

        let scan_album = {
            scan_artist
                .write()
                .await
                .add_album(
                    &album.title,
                    &album.release_date.clone(),
                    None,
                    &None,
                    &Some(album.id),
                )
                .await
        };
        {
            let read_album = { scan_album.read().await.clone() };

            {
                let read_artist = { scan_artist.read().await.clone() };

                if read_artist.cover.is_none() && !read_artist.searched_cover {
                    match moosicbox_tidal::artist(
                        db.clone(),
                        &album.artist_id.into(),
                        None,
                        None,
                        None,
                        None,
                    )
                    .await
                    {
                        Ok(artist) => {
                            if let Some(url) = artist.picture_url(TidalArtistImageSize::Max) {
                                scan_artist.write().await.search_cover(url, "tidal").await?;
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to get artist: {err:?}");
                        }
                    }
                }
            }

            if let Some(cover_url) = album.cover_url(TidalAlbumImageSize::Max) {
                if read_album.cover.is_none() && !read_album.searched_cover {
                    scan_album
                        .write()
                        .await
                        .search_cover(cover_url, "tidal")
                        .await?;
                }
            }
        }

        let limit = 100;
        let mut offset = 0;

        while !token.is_cancelled() {
            log::debug!(
                "Fetching Tidal tracks for album album_id={} offset={offset} limit={limit}",
                album.id
            );

            let album_id = &album.id.into();
            let tracks_resp = moosicbox_tidal::album_tracks(
                db.clone(),
                album_id,
                Some(offset),
                Some(limit),
                None,
                None,
                None,
                None,
            );

            select! {
                resp = tracks_resp => {
                    match resp {
                        Ok(page) => {
                            let page_count = page.len();

                            log::debug!("Fetched Tidal tracks offset={offset} limit={limit}: page_count={page_count}, total_count={}", page.total().unwrap());

                            scan_tracks(&page, scan_album.clone()).await?;

                            if !page.has_more() {
                                break;
                            }

                            offset += page_count as u32;
                        }
                        Err(err) =>  {
                            log::error!("Tidal scan error: {err:?}");
                            break;
                        }
                    }
                },
                _ = token.cancelled() => {
                    log::debug!("Cancelling Tidal scan");
                    return Ok(());
                }
            };
        }
    }

    Ok(())
}

pub async fn scan_tracks(
    tracks: &[TidalTrack],
    scan_album: Arc<RwLock<ScanAlbum>>,
) -> Result<(), ScanError> {
    log::debug!("Processing Tidal tracks count={}", tracks.len());

    for track in tracks {
        let _ = scan_album
            .write()
            .await
            .add_track(
                &None,
                track.track_number,
                &track.title,
                track.duration as f64,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                TrackApiSource::Tidal,
                &None,
                &Some(track.id),
            )
            .await;
    }

    Ok(())
}
