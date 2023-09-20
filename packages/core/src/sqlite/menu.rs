use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
    slim::{
        menu::{Album, AlbumSource},
        player::Track,
    },
};
use serde::{Deserialize, Serialize};
use std::{sync::PoisonError, time::Duration};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullAlbum {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub icon: Option<String>,
    pub source: AlbumSource,
}

impl<T> From<PoisonError<T>> for GetAlbumError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum GetAlbumError {
    #[error("Album not found with ID {album_id:?}")]
    AlbumNotFound { album_id: i32 },
    #[error("Too many albums found with ID {album_id:?}")]
    TooManyAlbumsFound { album_id: i32 },
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    SqliteError(#[from] sqlite::Error),
}

pub async fn get_album(
    player_id: &str,
    album_id: i32,
    data: &AppState,
) -> Result<FullAlbum, GetAlbumError> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("album|{player_id}|{proxy_url}|{album_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let results: Vec<_> = data
            .db
            .library
            .prepare("SELECT * from albums WHERE id = ?")?
            .into_iter()
            .bind((1, album_id as i64))
            .unwrap()
            .filter_map(|row| row.ok())
            .collect();

        if results.is_empty() {
            return Err(GetAlbumError::AlbumNotFound { album_id });
        }
        if results.len() > 1 {
            return Err(GetAlbumError::TooManyAlbumsFound { album_id });
        }

        let row = &results[0];

        let source = match row.read::<Option<&str>, _>("extid") {
            Some(ext_id) => {
                if ext_id.starts_with("qobuz:") {
                    AlbumSource::Qobuz
                } else if ext_id.starts_with("tidal:") {
                    AlbumSource::Tidal
                } else {
                    return Err(GetAlbumError::UnknownSource {
                        album_source: ext_id.to_string(),
                    });
                }
            }
            None => AlbumSource::Local,
        };

        let artist = String::from("");
        let title = String::from(row.read::<&str, _>("title"));
        let year = row.read::<Option<i64>, _>("year").map(|y| y as i32);
        let icon = row
            .read::<Option<&str>, _>("artwork")
            .map(|a| format!("/albums/{a}/300x300"));

        let album = FullAlbum {
            id: album_id,
            title,
            artist,
            year,
            icon,
            source,
        };

        Ok(CacheItemType::FullAlbum(album))
    })
    .await?
    .into_full_album()
    .unwrap())
}

impl<T> From<PoisonError<T>> for GetAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    JsonError(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    SqliteError(#[from] sqlite::Error),
}

pub async fn get_albums(data: &AppState) -> Result<Vec<Album>, GetAlbumsError> {
    let request = CacheRequest {
        key: "sqlite|local_albums".to_string(),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetAlbumsError>(CacheItemType::Albums(
            data.db
                .library
                .prepare("SELECT * from albums")?
                .into_iter()
                .filter_map(|row| row.ok())
                .map(|row| {
                    let id = String::from(row.read::<&str, _>("id"));
                    let artist = String::from(row.read::<&str, _>("artist"));
                    let title = String::from(row.read::<&str, _>("title"));
                    let date_released = row
                        .read::<Option<&str>, _>("date_released")
                        .map(|date| date.to_string());
                    let artwork = row
                        .read::<Option<&str>, _>("artwork")
                        .map(|_a| format!("/albums/{id}/300x300"));
                    let directory = row
                        .read::<Option<&str>, _>("directory")
                        .map(|dir| dir.to_string());
                    Album {
                        id,
                        title,
                        artist,
                        date_released,
                        artwork,
                        directory,
                        ..Default::default()
                    }
                })
                .collect(),
        ))
    })
    .await?
    .into_albums()
    .unwrap())
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    JsonError(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    SqliteError(#[from] sqlite::Error),
}

pub async fn get_album_tracks(
    album_id: i32,
    data: &AppState,
) -> Result<Vec<Track>, GetAlbumTracksError> {
    let request = CacheRequest {
        key: format!("sqlite|local_album_tracks|{album_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetAlbumTracksError>(CacheItemType::AlbumTracks(
            data.db
                .library
                .prepare("SELECT * from tracks WHERE album_id=?")?
                .into_iter()
                .bind((1, album_id as i64))?
                .filter_map(|row| row.ok())
                .map(|row| Track {
                    id: Some(row.read::<i64, _>("id") as i32),
                    title: String::from(row.read::<&str, _>("title")),
                    file: row.read::<Option<&str>, _>("file").map(|f| f.to_string()),
                    ..Default::default()
                })
                .collect(),
        ))
    })
    .await?
    .into_album_tracks()
    .unwrap())
}
