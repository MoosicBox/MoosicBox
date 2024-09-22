use std::sync::Arc;

use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Album, ApiSource, Track, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_files::FetchAndSaveBytesFromRemoteUrlError;
use moosicbox_music_api::{AlbumsError, AlbumsRequest, MusicApi};
use moosicbox_paging::PagingRequest;
use thiserror::Error;
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;

use crate::{
    output::{ScanAlbum, ScanOutput, UpdateDatabaseError},
    Scanner,
};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Albums(#[from] AlbumsError),
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

pub async fn scan(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    token: CancellationToken,
    scanner: Option<Scanner>,
) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let limit = 100;
    let mut offset = 0;

    loop {
        log::debug!("Fetching music api albums offset={offset} limit={limit}");

        let request = AlbumsRequest {
            sources: None,
            sort: None,
            filters: None,
            page: Some(PagingRequest { offset, limit }),
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

                        offset += page_count as u32;
                    }
                    Err(err) =>  {
                        log::warn!("music api scan error: {err:?}");
                        return Err(err.into());
                    }
                }
            },
            _ = token.cancelled() => {
                log::debug!("Cancelling music api scan");
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
        output.reindex_global_search_index(db).await?;
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

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
                .add_artist(&album.artist, &Some(&album.artist_id), api.source())
                .await
        };

        let scan_album = {
            scan_artist
                .write()
                .await
                .add_album(
                    album.title.as_str(),
                    &album.date_released.clone(),
                    None,
                    &Some(&album.id),
                    api.source(),
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
                                    .search_cover(url, api.source())
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

            if read_album.cover.is_none() && !read_album.searched_cover {
                if let Some(url) = album.artwork.clone() {
                    scan_album
                        .write()
                        .await
                        .search_cover(url, api.source())
                        .await?;
                }
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

                            offset += page_count as u32;
                        }
                        Err(err) =>  {
                            log::error!("music api scan error: {err:?}");
                            break;
                        }
                    }
                },
                _ = token.cancelled() => {
                    log::debug!("Cancelling music api scan");
                    return Ok(());
                }
            };
        }
    }

    Ok(())
}

async fn scan_tracks(
    api: &dyn MusicApi,
    tracks: &[Track],
    scan_album: Arc<RwLock<ScanAlbum>>,
    scanner: Option<Scanner>,
) -> Result<(), ScanError> {
    log::debug!("Processing music api tracks count={}", tracks.len());

    for track in tracks {
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
                match api.source() {
                    ApiSource::Library => unreachable!(),
                    ApiSource::Tidal => TrackApiSource::Tidal,
                    ApiSource::Qobuz => TrackApiSource::Qobuz,
                    ApiSource::Yt => TrackApiSource::Yt,
                },
                &Some(&track.id),
                api.source(),
            )
            .await;
        if let Some(scanner) = &scanner {
            scanner.on_scanned_track().await;
        }
    }

    Ok(())
}
