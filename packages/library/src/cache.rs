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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Arc;
    use thiserror::Error;

    #[derive(Debug, Error, PartialEq)]
    enum TestError {
        #[error("Test error: {0}")]
        TestError(String),
    }

    // Note: These tests must be run in serial to prevent race conditions with accessing the CACHE_MAP

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_computes_value_on_first_call() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_key",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                    crate::models::LibraryArtist {
                        id: 123,
                        title: "Test Artist".to_string(),
                        cover: None,
                        ..Default::default()
                    },
                )))
            },
        )
        .await;

        assert!(result.is_ok());
        let artist = result.unwrap().into_artist().unwrap();
        assert_eq!(artist.id, 123);
        assert_eq!(artist.title, "Test Artist");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_handles_errors() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_error",
                expiration: Duration::from_secs(60),
            },
            || async {
                Err::<CacheItemType, TestError>(TestError::TestError("Test error".to_string()))
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            TestError::TestError("Test error".to_string())
        );
    }

    #[test_log::test]
    #[serial]
    fn clear_cache_removes_all_entries() {
        clear_cache();

        // Add some entries via direct write to test clearing
        CACHE_MAP.write().unwrap().insert(
            "test_clear_1".to_string(),
            CacheItem {
                expiration: current_time_nanos() + 1_000_000_000,
                data: CacheItemType::Artist(Arc::new(crate::models::LibraryArtist {
                    id: 1,
                    title: "Test".to_string(),
                    cover: None,
                    ..Default::default()
                })),
            },
        );

        assert_eq!(CACHE_MAP.read().unwrap().len(), 1);

        clear_cache();

        assert_eq!(CACHE_MAP.read().unwrap().len(), 0);
    }

    #[test_log::test]
    fn current_time_nanos_returns_positive_value() {
        let time = current_time_nanos();
        assert!(time > 0);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_returns_cached_value_on_subsequent_calls() {
        clear_cache();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        // First call - should compute the value
        let call_count_clone = Arc::clone(&call_count);
        let result1 = get_or_set_to_cache(
            CacheRequest {
                key: "test_cache_hit",
                expiration: Duration::from_secs(60),
            },
            || {
                let count = Arc::clone(&call_count_clone);
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                        crate::models::LibraryArtist {
                            id: 456,
                            title: "Cached Artist".to_string(),
                            cover: None,
                            ..Default::default()
                        },
                    )))
                }
            },
        )
        .await;

        assert!(result1.is_ok());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Second call - should return cached value without computing
        let call_count_clone = Arc::clone(&call_count);
        let result2 = get_or_set_to_cache(
            CacheRequest {
                key: "test_cache_hit",
                expiration: Duration::from_secs(60),
            },
            || {
                let count = Arc::clone(&call_count_clone);
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                        crate::models::LibraryArtist {
                            id: 789, // Different ID to prove cache was used
                            title: "Different Artist".to_string(),
                            cover: None,
                            ..Default::default()
                        },
                    )))
                }
            },
        )
        .await;

        assert!(result2.is_ok());
        // Compute function should only have been called once
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Verify the cached value is returned (id should be 456, not 789)
        let artist = result2.unwrap().into_artist().unwrap();
        assert_eq!(artist.id, 456);
        assert_eq!(artist.title, "Cached Artist");
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    #[serial]
    async fn get_or_set_to_cache_recomputes_value_when_expired() {
        clear_cache();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        // First call - insert with a very short expiration
        let call_count_clone = Arc::clone(&call_count);
        let result1 = get_or_set_to_cache(
            CacheRequest {
                key: "test_expiration",
                expiration: Duration::from_nanos(1), // Very short expiration
            },
            || {
                let count = Arc::clone(&call_count_clone);
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                        crate::models::LibraryArtist {
                            id: 100,
                            title: "First Artist".to_string(),
                            cover: None,
                            ..Default::default()
                        },
                    )))
                }
            },
        )
        .await;

        assert!(result1.is_ok());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Wait a bit to ensure expiration
        switchy_async::time::sleep(Duration::from_millis(10)).await;

        // Second call - should recompute because entry is expired
        let call_count_clone = Arc::clone(&call_count);
        let result2 = get_or_set_to_cache(
            CacheRequest {
                key: "test_expiration",
                expiration: Duration::from_secs(60),
            },
            || {
                let count = Arc::clone(&call_count_clone);
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                        crate::models::LibraryArtist {
                            id: 200,
                            title: "Second Artist".to_string(),
                            cover: None,
                            ..Default::default()
                        },
                    )))
                }
            },
        )
        .await;

        assert!(result2.is_ok());
        // Compute function should have been called twice (once for initial, once for expired)
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);

        // Verify the new value is returned
        let artist = result2.unwrap().into_artist().unwrap();
        assert_eq!(artist.id, 200);
        assert_eq!(artist.title, "Second Artist");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_works_with_albums_type() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_albums",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::Albums(Arc::new(vec![
                    crate::models::LibraryAlbum {
                        id: 1,
                        title: "Album 1".to_string(),
                        artist: "Artist".to_string(),
                        ..Default::default()
                    },
                    crate::models::LibraryAlbum {
                        id: 2,
                        title: "Album 2".to_string(),
                        artist: "Artist".to_string(),
                        ..Default::default()
                    },
                ])))
            },
        )
        .await;

        assert!(result.is_ok());
        let albums = result.unwrap().into_albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].id, 1);
        assert_eq!(albums[1].id, 2);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_works_with_album_tracks_type() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_album_tracks",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::AlbumTracks(Arc::new(vec![
                    crate::models::LibraryTrack {
                        id: 1,
                        number: 1,
                        title: "Track 1".to_string(),
                        duration: 180.0,
                        album: "Test Album".to_string(),
                        album_id: 100,
                        ..Default::default()
                    },
                    crate::models::LibraryTrack {
                        id: 2,
                        number: 2,
                        title: "Track 2".to_string(),
                        duration: 200.0,
                        album: "Test Album".to_string(),
                        album_id: 100,
                        ..Default::default()
                    },
                ])))
            },
        )
        .await;

        assert!(result.is_ok());
        let tracks = result.unwrap().into_album_tracks().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].number, 1);
        assert_eq!(tracks[1].number, 2);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_works_with_artist_albums_type() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_artist_albums",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::ArtistAlbums(Arc::new(vec![
                    crate::models::LibraryAlbum {
                        id: 10,
                        title: "Artist Album 1".to_string(),
                        artist: "Same Artist".to_string(),
                        artist_id: 5,
                        ..Default::default()
                    },
                ])))
            },
        )
        .await;

        assert!(result.is_ok());
        let albums = result.unwrap().into_artist_albums().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].title, "Artist Album 1");
        assert_eq!(albums[0].artist_id, 5);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn get_or_set_to_cache_works_with_album_type() {
        clear_cache();

        let result = get_or_set_to_cache(
            CacheRequest {
                key: "test_single_album",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::Album(Arc::new(
                    crate::models::LibraryAlbum {
                        id: 42,
                        title: "Single Album".to_string(),
                        artist: "Solo Artist".to_string(),
                        date_released: Some("2024-01-01".to_string()),
                        ..Default::default()
                    },
                )))
            },
        )
        .await;

        assert!(result.is_ok());
        let album = result.unwrap().into_album().unwrap();
        assert_eq!(album.id, 42);
        assert_eq!(album.title, "Single Album");
        assert_eq!(album.date_released, Some("2024-01-01".to_string()));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn cache_preserves_distinct_keys_for_different_types() {
        clear_cache();

        // Cache an artist under "key_1"
        let artist_result = get_or_set_to_cache(
            CacheRequest {
                key: "distinct_key_artist",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::Artist(Arc::new(
                    crate::models::LibraryArtist {
                        id: 1,
                        title: "Artist Name".to_string(),
                        ..Default::default()
                    },
                )))
            },
        )
        .await;

        // Cache an album under "key_2"
        let album_result = get_or_set_to_cache(
            CacheRequest {
                key: "distinct_key_album",
                expiration: Duration::from_secs(60),
            },
            || async {
                Ok::<CacheItemType, TestError>(CacheItemType::Album(Arc::new(
                    crate::models::LibraryAlbum {
                        id: 2,
                        title: "Album Name".to_string(),
                        ..Default::default()
                    },
                )))
            },
        )
        .await;

        // Verify both are cached correctly and distinct
        assert!(artist_result.is_ok());
        assert!(album_result.is_ok());

        let artist = artist_result.unwrap().into_artist().unwrap();
        let album = album_result.unwrap().into_album().unwrap();

        assert_eq!(artist.id, 1);
        assert_eq!(album.id, 2);
        assert_eq!(CACHE_MAP.read().unwrap().len(), 2);
    }
}
