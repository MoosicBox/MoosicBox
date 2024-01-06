use std::{path::Path, sync::Arc};

use moosicbox_core::{
    app::Db,
    sqlite::{db::DbError, models::TrackSource},
    types::AudioFormat,
};
use moosicbox_tidal::{
    TidalAlbum, TidalAlbumTracksError, TidalArtistError, TidalFavoriteAlbumsError, TidalTrack,
};
use thiserror::Error;
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;

use crate::output::{
    sanitize_filename, FetchInternetImgError, ScanAlbum, ScanOutput, UpdateDatabaseError,
};

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
    FetchInternetImg(#[from] FetchInternetImgError),
}

pub async fn scan(db: &Db, token: CancellationToken) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let home_dir = home::home_dir().expect("Could not get user's home directory");
    let config_dir = home_dir.join(".local").join("moosicbox").join("cache");

    let limit = 100;
    let mut offset = 0;

    while !token.is_cancelled() {
        log::debug!("Fetching Tidal albums offset={offset} limit={limit}");

        let albums_resp = moosicbox_tidal::favorite_albums(
            db,
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
                    Ok((tidal_albums, count)) => {
                        let page_count = tidal_albums.len();

                        log::debug!("Fetched Tidal albums offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                        scan_albums(tidal_albums, count, db, &config_dir, output.clone(), token.clone()).await?;

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

    output.read().await.update_database(db).await?;

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

async fn scan_albums(
    albums: Vec<TidalAlbum>,
    total: u32,
    db: &Db,
    config_dir: &Path,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
) -> Result<(), ScanError> {
    log::debug!("Processing Tidal albums count={}", albums.len());

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

        let scan_artist = { output.write().await.add_artist(&album.artist).await };

        let scan_album = {
            scan_artist
                .write()
                .await
                .add_album(
                    &album.title,
                    &Some(album.release_date.clone()),
                    config_dir
                        .join(&sanitize_filename(&album.artist))
                        .join(&sanitize_filename(&album.title))
                        .to_str()
                        .unwrap(),
                )
                .await
        };
        {
            let read_album = { scan_album.read().await.clone() };

            {
                let read_artist = { scan_artist.read().await.clone() };

                if read_artist.cover.is_none() && !read_artist.searched_cover {
                    match moosicbox_tidal::artist(db, album.artist_id, None, None, None, None).await
                    {
                        Ok(artist) => {
                            if let Some(url) = artist.picture_url(750) {
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
                scan_album
                    .write()
                    .await
                    .search_cover(album.cover_url(1280))
                    .await?;
            }
        }

        let limit = 100;
        let mut offset = 0;

        while !token.is_cancelled() {
            log::debug!(
                "Fetching Tidal tracks for album album_id={} offset={offset} limit={limit}",
                album.id
            );

            let tracks_resp = moosicbox_tidal::album_tracks(
                db,
                album.id,
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
                        Ok((tidal_tracks, count)) => {
                            let page_count = tidal_tracks.len();

                            log::debug!("Fetched Tidal tracks offset={offset} limit={limit}: page_count={page_count}, total_count={count}");

                            scan_tracks(tidal_tracks, scan_album.clone()).await?;

                            if page_count < (limit as usize) {
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

async fn scan_tracks(
    tracks: Vec<TidalTrack>,
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
                0,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                TrackSource::Tidal,
            )
            .await;
    }

    Ok(())
}
