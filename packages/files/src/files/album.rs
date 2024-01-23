use std::path::Path;

use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album, DbError},
        models::AlbumId,
    },
};
use once_cell::sync::Lazy;
use thiserror::Error;

use crate::{
    fetch_and_save_bytes_from_remote_url, sanitize_filename, FetchAndSaveBytesFromRemoteUrlError,
};

pub enum AlbumCoverSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum FetchAlbumCoverError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

async fn get_or_fetch_album_cover_from_remote_url(
    url: &str,
    artist_name: &str,
    album_name: &str,
) -> Result<String, FetchAlbumCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    // This path might overwrite existing library album.jpg
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(sanitize_filename(artist_name))
        .join(sanitize_filename(album_name));

    let filename = "album.jpg";
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
pub enum AlbumCoverError {
    #[error("Album cover not found for album: {0:?}")]
    NotFound(AlbumId),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    FetchAlbumCover(#[from] FetchAlbumCoverError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TidalAlbum(#[from] moosicbox_tidal::TidalAlbumError),
    #[error(transparent)]
    QobuzAlbum(#[from] moosicbox_qobuz::QobuzAlbumError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

pub async fn get_album_cover(
    album_id: AlbumId,
    db: Db,
) -> Result<AlbumCoverSource, AlbumCoverError> {
    let path = match &album_id {
        AlbumId::Library(library_album_id) => {
            let album = get_album(
                &db.library.lock().as_ref().unwrap().inner,
                *library_album_id,
            )?
            .ok_or(AlbumCoverError::NotFound(album_id.clone()))?;
            let cover = album
                .artwork
                .ok_or(AlbumCoverError::NotFound(album_id.clone()))?;
            let directory = album.directory.ok_or(AlbumCoverError::InvalidSource)?;

            std::path::PathBuf::from(directory)
                .join(cover)
                .to_str()
                .unwrap()
                .to_string()
        }
        AlbumId::Tidal(tidal_album_id) => {
            let album =
                moosicbox_tidal::album(&db, *tidal_album_id, None, None, None, None).await?;
            get_or_fetch_album_cover_from_remote_url(
                &album.cover_url(1280),
                &album.artist,
                &album.title,
            )
            .await?
        }
        AlbumId::Qobuz(qobuz_album_id) => {
            let album = moosicbox_qobuz::album(&db, qobuz_album_id, None, None).await?;
            let cover = album
                .cover_url()
                .ok_or(AlbumCoverError::NotFound(album_id.clone()))?;
            get_or_fetch_album_cover_from_remote_url(&cover, &album.artist, &album.title).await?
        }
    };

    Ok(AlbumCoverSource::LocalFilePath(path))
}
