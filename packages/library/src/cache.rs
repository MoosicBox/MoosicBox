#![allow(clippy::module_name_repetitions)]

use enum_as_inner::EnumAsInner;
use futures::Future;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::{LibraryAlbum, LibraryArtist, LibraryTrack};

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

/// # Panics
///
/// * If time went backwards
#[must_use]
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

static CACHE_MAP: LazyLock<RwLock<HashMap<String, CacheItem>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// # Panics
///
/// * If `RwLock` is poisoned
pub fn clear_cache() {
    CACHE_MAP.write().unwrap().clear();
}

/// # Panics
///
/// * If `RwLock` is poisoned
///
/// # Errors
///
/// * If the `compute` `Fn` fails
pub async fn get_or_set_to_cache<Fut, Err>(
    request: CacheRequest<'_>,
    compute: impl Fn() -> Fut + Send,
) -> Result<CacheItemType, Err>
where
    Err: Error,
    Fut: Future<Output = Result<CacheItemType, Err>> + Send,
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
