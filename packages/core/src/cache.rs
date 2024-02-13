use crate::sqlite::models::{LibraryAlbum, LibraryArtist, LibraryTrack};
use enum_as_inner::EnumAsInner;
use futures::Future;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CacheItem {
    expiration: u128,
    data: CacheItemType,
}

#[derive(Debug, Serialize, Deserialize, Clone, EnumAsInner)]
#[serde(untagged)]
pub enum CacheItemType {
    Albums(Arc<Vec<LibraryAlbum>>),
    AlbumTracks(Arc<Vec<LibraryTrack>>),
    ArtistAlbums(Arc<Vec<LibraryAlbum>>),
    Artist(Arc<LibraryArtist>),
    Album(Arc<LibraryAlbum>),
}

pub fn current_time_nanos() -> u128 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_nanos()
}

pub struct CacheRequest<'a> {
    pub key: &'a str,
    pub expiration: Duration,
}
static CACHE_MAP: Lazy<RwLock<HashMap<String, CacheItem>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn clear_cache() {
    CACHE_MAP.write().unwrap().clear();
}

pub async fn get_or_set_to_cache<Fut, Err>(
    request: CacheRequest<'_>,
    compute: impl Fn() -> Fut,
) -> Result<CacheItemType, Err>
where
    Err: Error,
    Fut: Future<Output = Result<CacheItemType, Err>>,
{
    if let Some(entry) = CACHE_MAP.read().unwrap().get(request.key) {
        if entry.expiration > current_time_nanos() {
            return Ok(entry.data.clone());
        }
    }

    let value = match compute().await {
        Ok(x) => x,
        Err(error) => return Err(error),
    };

    CACHE_MAP.write().unwrap().insert(
        request.key.to_string(),
        CacheItem {
            expiration: current_time_nanos() + request.expiration.as_nanos(),
            data: value.clone(),
        },
    );

    Ok(value)
}
