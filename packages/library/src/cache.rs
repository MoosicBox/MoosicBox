//! Caching functionality for library data to improve performance.
//!
//! This module provides a simple in-memory cache for library items such as albums,
//! tracks, and artists. The cache supports expiration times and automatic cleanup.

#![allow(clippy::module_name_repetitions)]

use enum_as_inner::EnumAsInner;
use futures::Future;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, UNIX_EPOCH};

use crate::models::{LibraryAlbum, LibraryArtist, LibraryTrack};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CacheItem {
    expiration: u128,
    data: CacheItemType,
}

/// Types of cacheable library items.
#[derive(Debug, Serialize, Deserialize, Clone, EnumAsInner)]
#[serde(untagged)]
pub enum CacheItemType {
    /// Cached list of albums.
    Albums(Arc<Vec<LibraryAlbum>>),
    /// Cached list of album tracks.
    AlbumTracks(Arc<Vec<LibraryTrack>>),
    /// Cached list of artist albums.
    ArtistAlbums(Arc<Vec<LibraryAlbum>>),
    /// Cached artist.
    Artist(Arc<LibraryArtist>),
    /// Cached album.
    Album(Arc<LibraryAlbum>),
}

/// Returns the current time in nanoseconds since the Unix epoch.
///
/// # Panics
///
/// * If time went backwards
#[must_use]
pub fn current_time_nanos() -> u128 {
    let start = switchy_time::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_nanos()
}

/// Request parameters for cache operations.
#[derive(Debug)]
pub struct CacheRequest<'a> {
    /// Cache key.
    pub key: &'a str,
    /// Cache entry expiration duration.
    pub expiration: Duration,
}

static CACHE_MAP: LazyLock<RwLock<BTreeMap<String, CacheItem>>> =
    LazyLock::new(|| RwLock::new(BTreeMap::new()));

/// Clears all entries from the cache.
///
/// # Panics
///
/// * If `RwLock` is poisoned
pub fn clear_cache() {
    CACHE_MAP.write().unwrap().clear();
}

/// Retrieves a value from cache or computes and caches it if not present or expired.
///
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
    if let Some(entry) = CACHE_MAP.read().unwrap().get(request.key)
        && entry.expiration > current_time_nanos()
    {
        return Ok(entry.data.clone());
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
