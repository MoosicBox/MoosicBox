use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
    menu::AlbumSource,
};

use std::{error::Error, fmt, time::Duration};

use actix_web::web;
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullAlbum {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub icon: Option<String>,
    pub source: AlbumSource,
}

#[derive(Debug, Clone)]
pub struct AlbumNotFound {
    details: String,
}
impl AlbumNotFound {
    fn new(msg: Option<&str>) -> AlbumNotFound {
        AlbumNotFound {
            details: msg.unwrap_or("Album not found").to_string(),
        }
    }
}
impl fmt::Display for AlbumNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}
impl Error for AlbumNotFound {
    fn description(&self) -> &str {
        &self.details
    }
}

#[derive(Debug, Clone)]
pub struct TooManyAlbumsFound {
    details: String,
}
impl TooManyAlbumsFound {
    fn new(msg: Option<&str>) -> TooManyAlbumsFound {
        TooManyAlbumsFound {
            details: msg.unwrap_or("Too many albums found").to_string(),
        }
    }
}
impl fmt::Display for TooManyAlbumsFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}
impl Error for TooManyAlbumsFound {
    fn description(&self) -> &str {
        &self.details
    }
}

#[derive(Debug, Clone)]
pub struct UnknownSource {
    details: String,
}
impl UnknownSource {
    fn new(source: &str) -> UnknownSource {
        UnknownSource {
            details: format!("Unknown source '{source}'"),
        }
    }
}
impl fmt::Display for UnknownSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}
impl Error for UnknownSource {
    fn description(&self) -> &str {
        &self.details
    }
}

pub async fn get_album(
    player_id: &str,
    album_id: i32,
    data: web::Data<AppState>,
) -> Result<FullAlbum> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("album|{player_id}|{proxy_url}|{album_id}"),
        expiration: Duration::from_secs(60 * 60),
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
            return Err(AlbumNotFound::new(None).into());
        }
        if results.len() > 1 {
            return Err(TooManyAlbumsFound::new(None).into());
        }

        let row = &results[0];

        let source = match row.read::<Option<&str>, _>("extid") {
            Some(ext_id) => {
                if ext_id.starts_with("qobuz:") {
                    AlbumSource::Qobuz
                } else if ext_id.starts_with("tidal:") {
                    AlbumSource::Tidal
                } else {
                    return Err(UnknownSource::new(ext_id).into());
                }
            }
            None => AlbumSource::Local,
        };

        let artist = String::from("");
        let title = String::from(row.read::<&str, _>("title"));
        let year = row.read::<Option<i64>, _>("year").map(|y| y as i32);
        let icon = row
            .read::<Option<&str>, _>("artwork")
            .map(|a| format!("albums/{a}/300x300"));

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
