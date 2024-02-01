use std::{collections::HashMap, path::Path, sync::RwLock};

use async_recursion::async_recursion;
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_artist, DbError, SqliteValue},
        models::{
            qobuz::{QobuzArtist, QobuzImageSize},
            tidal::{TidalArtist, TidalImageSize},
            ArtistId, LibraryArtist,
        },
    },
};
use moosicbox_qobuz::QobuzArtistError;
use moosicbox_tidal::TidalArtistError;
use once_cell::sync::Lazy;
use thiserror::Error;

use crate::{
    fetch_and_save_bytes_from_remote_url, sanitize_filename, FetchAndSaveBytesFromRemoteUrlError,
};

pub enum ArtistCoverSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum FetchArtistCoverError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

async fn get_or_fetch_artist_cover_from_remote_url(
    url: &str,
    size: &str,
    source: &str,
    artist_name: &str,
) -> Result<String, FetchArtistCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name));

    let filename = format!("artist_{size}.jpg");
    let file_path = path.join(filename);

    if Path::exists(&file_path) {
        Ok(file_path.to_str().unwrap().to_string())
    } else {
        Ok(
            fetch_and_save_bytes_from_remote_url(&IMAGE_CLIENT, &file_path, url)
                .await?
                .to_str()
                .unwrap()
                .to_string(),
        )
    }
}

#[derive(Debug, Error)]
pub enum FetchLocalArtistCoverError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No Artist Cover")]
    NoArtistCover,
}

fn fetch_local_artist_cover(cover: Option<String>) -> Result<String, FetchLocalArtistCoverError> {
    let cover = cover.ok_or(FetchLocalArtistCoverError::NoArtistCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::exists(&cover_path) {
        return Ok(cover_path.to_str().unwrap().to_string());
    }

    Err(FetchLocalArtistCoverError::NoArtistCover)
}

#[derive(Debug, Error)]
pub enum ArtistCoverError {
    #[error("Artist cover not found for artist: {0:?}")]
    NotFound(ArtistId),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    FetchArtistCover(#[from] FetchArtistCoverError),
    #[error(transparent)]
    TidalArtist(#[from] moosicbox_tidal::TidalArtistError),
    #[error(transparent)]
    QobuzArtist(#[from] moosicbox_qobuz::QobuzArtistError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

fn copy_streaming_cover_to_local(
    db: &Db,
    artist_id: i32,
    cover: String,
) -> Result<String, ArtistCoverError> {
    log::debug!("Updating Artist {artist_id} cover file to '{cover}'");

    moosicbox_core::sqlite::db::update_and_get_row::<LibraryArtist>(
        &db.library.lock().as_ref().unwrap().inner,
        "artists",
        SqliteValue::Number(artist_id as i64),
        &[("cover", SqliteValue::String(cover.to_string()))],
    )?;

    Ok(cover)
}

#[async_recursion]
pub async fn get_artist_cover(
    artist_id: ArtistId,
    db: &Db,
    size: Option<u32>,
) -> Result<ArtistCoverSource, ArtistCoverError> {
    let path = match &artist_id {
        ArtistId::Library(library_artist_id) => {
            let artist = get_artist(
                &db.library.lock().as_ref().unwrap().inner,
                *library_artist_id,
            )?
            .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;

            if let Ok(cover) = fetch_local_artist_cover(artist.cover) {
                return Ok(ArtistCoverSource::LocalFilePath(cover));
            }

            if let Some(tidal_id) = artist.tidal_id {
                if let Ok(ArtistCoverSource::LocalFilePath(cover)) =
                    get_artist_cover(ArtistId::Tidal(tidal_id), db, None).await
                {
                    return Ok(ArtistCoverSource::LocalFilePath(
                        copy_streaming_cover_to_local(db, artist.id, cover)?,
                    ));
                }
            }

            if let Some(qobuz_id) = artist.qobuz_id {
                if let Ok(ArtistCoverSource::LocalFilePath(cover)) =
                    get_artist_cover(ArtistId::Qobuz(qobuz_id), db, None).await
                {
                    return Ok(ArtistCoverSource::LocalFilePath(
                        copy_streaming_cover_to_local(db, artist.id, cover)?,
                    ));
                }
            }

            return Err(ArtistCoverError::NotFound(artist_id));
        }
        ArtistId::Tidal(tidal_artist_id) => {
            static ARTIST_CACHE: Lazy<RwLock<HashMap<u64, Option<TidalArtist>>>> =
                Lazy::new(|| RwLock::new(HashMap::new()));

            let artist = if let Some(artist) = {
                let binding = ARTIST_CACHE.read().unwrap();
                binding.get(tidal_artist_id).cloned()
            } {
                artist
            } else {
                use moosicbox_tidal::AuthenticatedRequestError;

                let artist = match moosicbox_tidal::artist(
                    db,
                    &tidal_artist_id.into(),
                    None,
                    None,
                    None,
                    None,
                )
                .await
                {
                    Ok(album) => Ok(Some(album)),
                    Err(err) => match err {
                        TidalArtistError::AuthenticatedRequest(
                            AuthenticatedRequestError::RequestFailed(404, _),
                        ) => Ok(None),
                        _ => Err(err),
                    },
                }?;

                ARTIST_CACHE
                    .write()
                    .as_mut()
                    .unwrap()
                    .insert(*tidal_artist_id, artist.clone());

                artist
            }
            .ok_or_else(|| ArtistCoverError::NotFound(artist_id.clone()))?;

            let size = size
                .map(|size| (size as u16).into())
                .unwrap_or(TidalImageSize::Max);

            let cover = artist
                .picture_url(size)
                .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;

            get_or_fetch_artist_cover_from_remote_url(
                &cover,
                &size.to_string(),
                "tidal",
                &artist.name,
            )
            .await?
        }
        ArtistId::Qobuz(qobuz_artist_id) => {
            static ARTIST_CACHE: Lazy<RwLock<HashMap<u64, Option<QobuzArtist>>>> =
                Lazy::new(|| RwLock::new(HashMap::new()));

            let artist = if let Some(artist) = {
                let binding = ARTIST_CACHE.read().unwrap();
                binding.get(qobuz_artist_id).cloned()
            } {
                artist
            } else {
                use moosicbox_qobuz::AuthenticatedRequestError;

                let artist =
                    match moosicbox_qobuz::artist(db, &qobuz_artist_id.into(), None, None).await {
                        Ok(album) => Ok(Some(album)),
                        Err(err) => match err {
                            QobuzArtistError::AuthenticatedRequest(
                                AuthenticatedRequestError::RequestFailed(404, _),
                            ) => Ok(None),
                            _ => Err(err),
                        },
                    }?;

                ARTIST_CACHE
                    .write()
                    .as_mut()
                    .unwrap()
                    .insert(*qobuz_artist_id, artist.clone());

                artist
            }
            .ok_or_else(|| ArtistCoverError::NotFound(artist_id.clone()))?;

            let size = size
                .map(|size| (size as u16).into())
                .unwrap_or(QobuzImageSize::Mega);

            let cover = artist
                .image
                .as_ref()
                .and_then(|image| image.cover_url_for_size(size))
                .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;

            get_or_fetch_artist_cover_from_remote_url(
                &cover,
                &size.to_string(),
                "qobuz",
                &artist.name,
            )
            .await?
        }
    };

    Ok(ArtistCoverSource::LocalFilePath(path))
}
