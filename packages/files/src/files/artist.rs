use std::path::Path;

use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_artist, DbError},
        models::ArtistId,
    },
};
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
    artist_name: &str,
) -> Result<String, FetchArtistCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    // This path might overwrite existing library artist.jpg
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(sanitize_filename(artist_name));

    let filename = "artist.jpg";
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
pub enum ArtistCoverError {
    #[error("Artist cover not found for album: {0:?}")]
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

pub async fn get_artist_cover(
    artist_id: ArtistId,
    db: Db,
) -> Result<ArtistCoverSource, ArtistCoverError> {
    let path = match &artist_id {
        ArtistId::Library(library_artist_id) => {
            let artist = get_artist(
                &db.library.lock().as_ref().unwrap().inner,
                *library_artist_id,
            )?
            .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;
            let cover = artist
                .cover
                .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;

            cover
        }
        ArtistId::Tidal(tidal_artist_id) => {
            let artist =
                moosicbox_tidal::artist(&db, *tidal_artist_id, None, None, None, None).await?;
            let cover = artist
                .picture_url(750)
                .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;
            get_or_fetch_artist_cover_from_remote_url(&cover, &artist.name).await?
        }
        ArtistId::Qobuz(qobuz_artist_id) => {
            let artist = moosicbox_qobuz::artist(&db, *qobuz_artist_id, None, None).await?;
            let cover = artist
                .cover_url()
                .ok_or(ArtistCoverError::NotFound(artist_id.clone()))?;
            get_or_fetch_artist_cover_from_remote_url(&cover, &artist.name).await?
        }
    };

    Ok(ArtistCoverSource::LocalFilePath(path))
}
