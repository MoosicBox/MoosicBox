use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
    slim::menu::AlbumSource,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
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

#[derive(Debug, Error)]
pub enum GetAlbumError {
    #[error("Album not found with ID {album_id:?}")]
    AlbumNotFound { album_id: i32 },
    #[error("Too many albums found with ID {album_id:?}")]
    TooManyAlbumsFound { album_id: i32 },
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
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
        expiration: Duration::from_secs(60 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let results: Vec<_> = data
            .db
            .as_ref()
            .unwrap()
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
