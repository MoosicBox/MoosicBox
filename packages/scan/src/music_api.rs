//! Music API scanning functionality for remote streaming services.
//!
//! This module provides functions to scan music libraries from remote music API services
//! (e.g., Tidal, Qobuz) by fetching albums, tracks, and artwork metadata.

use std::sync::Arc;

use moosicbox_files::FetchAndSaveBytesFromRemoteUrlError;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::{MusicApi, models::AlbumsRequest};
use moosicbox_music_models::{Album, AudioFormat, Track};
use moosicbox_paging::PagingRequest;
use switchy_async::util::CancellationToken;
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;
use tokio::{select, sync::RwLock};

use crate::{
    Scanner,
    output::{ScanAlbum, ScanOutput, UpdateDatabaseError},
};

/// Errors that can occur during music API scanning.
#[derive(Debug, Error)]
pub enum ScanError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API operation failed.
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Database update operation failed.
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
    /// Failed to fetch and save bytes from remote URL.
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

/// Scans a music API origin for all albums and tracks.
///
/// # Panics
///
/// * If the page total is missing
/// * If the page count is outside the range of a `u32`
///
/// # Errors
///
/// * If the scan fails
pub async fn scan(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    token: CancellationToken,
    scanner: Option<Scanner>,
) -> Result<(), ScanError> {
    let total_start = switchy_time::now();
    let start = switchy_time::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let limit = 100;
    let mut offset = 0;

    loop {
        log::debug!("Fetching music api albums offset={offset} limit={limit}");

        let request = AlbumsRequest {
            page: Some(PagingRequest { offset, limit }),
            ..Default::default()
        };

        let albums_resp = api.albums(&request);

        select! {
            resp = albums_resp => {
                match resp {
                    Ok(page) => {
                        let page_count = page.len();
                        let count = page.total().unwrap();

                        log::debug!("Fetched music api albums offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                        scan_albums(api, &page, count, output.clone(), Some(token.clone()), scanner.clone()).await?;

                        if page_count < (limit as usize) {
                            break;
                        }

                        offset += u32::try_from(page_count).unwrap();
                    }
                    Err(err) =>  {
                        log::warn!("music api scan error: {err:?}");
                        return Err(err.into());
                    }
                }
            },
            () = token.cancelled() => {
                log::debug!("Cancelling music api scan");
                return Ok(());
            }
        };
    }

    let end = switchy_time::now();
    log::info!(
        "Finished initial scan in {}ms",
        end.duration_since(start).unwrap().as_millis()
    );

    let output = output.read().await;
    output.update_database(db).await?;
    output.reindex_global_search_index(db).await?;
    drop(output);

    let end = switchy_time::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

/// Scans a batch of albums from a music API, fetching tracks and artwork.
///
/// # Panics
///
/// * If the page total is missing
/// * If the page count is outside the range of a `u32`
///
/// # Errors
///
/// * If the albums scan fails
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub async fn scan_albums(
    api: &dyn MusicApi,
    albums: &[Album],
    total: u32,
    output: Arc<RwLock<ScanOutput>>,
    token: Option<CancellationToken>,
    scanner: Option<Scanner>,
) -> Result<(), ScanError> {
    log::debug!("Processing music api albums count={}", albums.len());

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
                .add_artist(&album.artist, &Some(&album.artist_id), api.source().clone())
                .await
        };

        let scan_album = {
            scan_artist
                .write()
                .await
                .add_album(
                    album.title.as_str(),
                    &album.date_released.map(|x| x.and_utc().to_rfc3339()),
                    None,
                    &Some(&album.id),
                    api.source().clone(),
                )
                .await
        };
        {
            let read_album = { scan_album.read().await.clone() };

            {
                let read_artist = { scan_artist.read().await.clone() };

                if read_artist.cover.is_none() && !read_artist.searched_cover {
                    match api.artist(&album.artist_id).await {
                        Ok(Some(artist)) => {
                            if let Some(url) = artist.cover {
                                scan_artist
                                    .write()
                                    .await
                                    // FIXME: Pass headers
                                    .search_cover(url, None, api.source())
                                    .await?;
                            }
                        }
                        Ok(None) => {
                            log::warn!("Failed to get artist: (no artist)");
                        }
                        Err(err) => {
                            log::warn!("Failed to get artist: {err:?}");
                        }
                    }
                }
            }

            if read_album.cover.is_none()
                && !read_album.searched_cover
                && let Some(url) = album.artwork.clone()
            {
                scan_album
                    .write()
                    .await
                    // FIXME: Pass headers
                    .search_cover(url, None, api.source())
                    .await?;
            }
        }

        let limit = 100;
        let mut offset = 0;

        loop {
            log::debug!(
                "Fetching music api tracks for album album_id={} offset={offset} limit={limit}",
                album.id
            );

            let album_id = &album.id;
            let tracks_resp = api.album_tracks(album_id, Some(offset), Some(limit), None, None);

            select! {
                resp = tracks_resp => {
                    match resp {
                        Ok(page) => {
                            let page_count = page.len();
                            let count = page.total().unwrap();
                            if let Some(scanner) = &scanner {
                                scanner.increase_total(count as usize).await;
                            }

                            log::debug!("Fetched music api tracks offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                            scan_tracks(api, &page, scan_album.clone(), scanner.clone()).await?;

                            if page_count < (limit as usize) {
                                break;
                            }

                            offset += u32::try_from(page_count).unwrap();
                        }
                        Err(err) =>  {
                            log::error!("music api scan error: {err:?}");
                            break;
                        }
                    }
                },
                () = token.cancelled() => {
                    log::debug!("Cancelling music api scan");
                    return Ok(());
                }
            };
        }
    }

    Ok(())
}

/// Scans a batch of tracks from a music API album.
///
/// # Errors
///
/// * If the tracks scan fails
async fn scan_tracks(
    api: &dyn MusicApi,
    tracks: &[Track],
    scan_album: Arc<RwLock<ScanAlbum>>,
    #[allow(unused)] scanner: Option<Scanner>,
) -> Result<(), ScanError> {
    log::debug!("Processing music api tracks count={}", tracks.len());

    let source = api.source();

    if source.is_library() {
        moosicbox_assert::die!("Invalid api source");
        return Ok(());
    }

    for track in tracks {
        #[allow(unreachable_code)]
        let _ = scan_album
            .write()
            .await
            .add_track(
                &None,
                track.number,
                track.title.as_str(),
                track.duration,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                source.clone().into(),
                &Some(&track.id),
                source.clone(),
            )
            .await;
        if let Some(scanner) = &scanner {
            scanner.on_scanned_track().await;
        }
    }

    Ok(())
}
