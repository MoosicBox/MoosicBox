use moosicbox_core::{app::Db, sqlite::db::DbError};
use moosicbox_tidal::{TidalAlbum, TidalFavoriteAlbumsError};
use thiserror::Error;
use tokio::select;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Tidal(#[from] TidalFavoriteAlbumsError),
}

pub async fn scan(db: &Db, token: CancellationToken) -> Result<(), ScanError> {
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

                        scan_albums(tidal_albums, db, token.clone())?;

                        if page_count < (limit as usize) {
                            break;
                        }

                        offset += page_count as u32;
                    }
                    Err(err) =>  {
                        log::error!("Tidal scan error: {err:?}");
                        return Err(ScanError::Tidal(err));
                    }
                }
            },
            _ = token.cancelled() => {
                log::debug!("Cancelling Tidal scan");
                return Ok(());
            }
        };
    }

    Ok(())
}

fn scan_albums(
    albums: Vec<TidalAlbum>,
    _db: &Db,
    _token: CancellationToken,
) -> Result<(), ScanError> {
    log::debug!("Processing Tidal albums count={}", albums.len());

    Ok(())
}
