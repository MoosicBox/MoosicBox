#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::similar_names)]

//! Basic usage example for `moosicbox_music_api`
//!
//! This example demonstrates:
//! - Implementing the `MusicApi` trait
//! - Using the `CachedMusicApi` wrapper
//! - Working with artists, albums, and tracks
//! - Managing collections with `MusicApis`

use async_trait::async_trait;
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    CachedMusicApi, Error, MusicApi, MusicApis, TrackOrId,
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
    },
};
use moosicbox_music_models::{Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track, id::Id};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use std::sync::Arc;

/// A simple in-memory music API implementation for demonstration purposes
struct SimpleMusicApi {
    source: ApiSource,
    artists: Vec<Artist>,
    albums: Vec<Album>,
    tracks: Vec<Track>,
}

impl SimpleMusicApi {
    /// Creates a new simple music API with sample data
    fn new() -> Self {
        // Register a custom API source
        let source = ApiSource::register("simple", "Simple Music API");

        // Create sample artists
        let artists = vec![
            Artist {
                id: Id::Number(1),
                title: "The Beatles".to_string(),
                cover: Some("https://example.com/beatles.jpg".to_string()),
                ..Default::default()
            },
            Artist {
                id: Id::Number(2),
                title: "Pink Floyd".to_string(),
                cover: Some("https://example.com/pinkfloyd.jpg".to_string()),
                ..Default::default()
            },
        ];

        // Create sample albums
        let albums = vec![
            Album {
                id: Id::Number(1),
                title: "Abbey Road".to_string(),
                artist: "The Beatles".to_string(),
                artist_id: Id::Number(1),
                artwork: Some("https://example.com/abbey-road.jpg".to_string()),
                ..Default::default()
            },
            Album {
                id: Id::Number(2),
                title: "The Dark Side of the Moon".to_string(),
                artist: "Pink Floyd".to_string(),
                artist_id: Id::Number(2),
                artwork: Some("https://example.com/dark-side.jpg".to_string()),
                ..Default::default()
            },
        ];

        // Create sample tracks
        let tracks = vec![
            Track {
                id: Id::Number(1),
                title: "Come Together".to_string(),
                album: "Abbey Road".to_string(),
                album_id: Id::Number(1),
                artist: "The Beatles".to_string(),
                artist_id: Id::Number(1),
                ..Default::default()
            },
            Track {
                id: Id::Number(2),
                title: "Time".to_string(),
                album: "The Dark Side of the Moon".to_string(),
                album_id: Id::Number(2),
                artist: "Pink Floyd".to_string(),
                artist_id: Id::Number(2),
                ..Default::default()
            },
        ];

        Self {
            source,
            artists,
            albums,
            tracks,
        }
    }
}

#[async_trait]
impl MusicApi for SimpleMusicApi {
    fn source(&self) -> &ApiSource {
        &self.source
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<ArtistOrder>,
        _order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, Error> {
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(10) as usize;

        let items = self
            .artists
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset: offset as u32,
                limit: limit as u32,
                total: self.artists.len() as u32,
            },
            |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error> {
        Ok(self.artists.iter().find(|a| &a.id == artist_id).cloned())
    }

    async fn add_artist(&self, _artist_id: &Id) -> Result<(), Error> {
        println!("Adding artist (not implemented in example)");
        Ok(())
    }

    async fn remove_artist(&self, _artist_id: &Id) -> Result<(), Error> {
        println!("Removing artist (not implemented in example)");
        Ok(())
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, Error> {
        let offset = request
            .page
            .as_ref()
            .map_or(0, |p| p.offset) as usize;
        let limit = request
            .page
            .as_ref()
            .map_or(10, |p| p.limit) as usize;

        let items = self
            .albums
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset: offset as u32,
                limit: limit as u32,
                total: self.albums.len() as u32,
            },
            |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error> {
        Ok(self.albums.iter().find(|a| &a.id == album_id).cloned())
    }

    async fn album_versions(
        &self,
        _album_id: &Id,
        _offset: Option<u32>,
        _limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, Error> {
        Ok(PagingResponse::empty())
    }

    async fn artist_albums(
        &self,
        artist_id: &Id,
        _album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, Error> {
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(10) as usize;

        let items: Vec<Album> = self
            .albums
            .iter()
            .filter(|a| &a.artist_id == artist_id)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        let total = self
            .albums
            .iter()
            .filter(|a| &a.artist_id == artist_id)
            .count() as u32;

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset: offset as u32,
                limit: limit as u32,
                total,
            },
            |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn add_album(&self, _album_id: &Id) -> Result<(), Error> {
        println!("Adding album (not implemented in example)");
        Ok(())
    }

    async fn remove_album(&self, _album_id: &Id) -> Result<(), Error> {
        println!("Removing album (not implemented in example)");
        Ok(())
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error> {
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(10) as usize;

        let items = if let Some(ids) = track_ids {
            self.tracks
                .iter()
                .filter(|t| ids.contains(&t.id))
                .skip(offset)
                .take(limit)
                .cloned()
                .collect()
        } else {
            self.tracks
                .iter()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect()
        };

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset: offset as u32,
                limit: limit as u32,
                total: self.tracks.len() as u32,
            },
            |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error> {
        Ok(self.tracks.iter().find(|t| &t.id == track_id).cloned())
    }

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error> {
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(10) as usize;

        let items: Vec<Track> = self
            .tracks
            .iter()
            .filter(|t| &t.album_id == album_id)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        let total = self
            .tracks
            .iter()
            .filter(|t| &t.album_id == album_id)
            .count() as u32;

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset: offset as u32,
                limit: limit as u32,
                total,
            },
            |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn add_track(&self, _track_id: &Id) -> Result<(), Error> {
        println!("Adding track (not implemented in example)");
        Ok(())
    }

    async fn remove_track(&self, _track_id: &Id) -> Result<(), Error> {
        println!("Removing track (not implemented in example)");
        Ok(())
    }

    async fn track_source(
        &self,
        _track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, Error> {
        // In a real implementation, this would return the actual track source
        Ok(None)
    }

    async fn track_size(
        &self,
        _track: TrackOrId,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, Error> {
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Music API - Basic Usage Example ===\n");

    // Step 1: Create a simple music API implementation
    println!("Step 1: Creating a simple music API implementation...");
    let api = SimpleMusicApi::new();
    println!("API source: {:?}\n", api.source());

    // Step 2: Fetch all artists
    println!("Step 2: Fetching all artists...");
    let artists = api.artists(Some(0), Some(10), None, None).await?;
    println!("Found {} artists:", artists.len());
    for artist in &artists[..] {
        println!("  - {} (ID: {:?})", artist.title, artist.id);
    }
    println!();

    // Step 3: Fetch a specific artist by ID
    println!("Step 3: Fetching artist with ID 1...");
    let artist = api.artist(&Id::Number(1)).await?;
    if let Some(artist) = artist {
        println!("Found artist: {}", artist.title);
        println!("Cover: {:?}\n", artist.cover);
    }

    // Step 4: Fetch albums for an artist
    println!("Step 4: Fetching albums for artist ID 1...");
    let albums = api
        .artist_albums(&Id::Number(1), None, Some(0), Some(10), None, None)
        .await?;
    println!("Found {} albums:", albums.len());
    for album in &albums[..] {
        println!("  - {} by {}", album.title, album.artist);
    }
    println!();

    // Step 5: Fetch tracks for an album
    println!("Step 5: Fetching tracks for album ID 1...");
    let tracks = api
        .album_tracks(&Id::Number(1), Some(0), Some(10), None, None)
        .await?;
    println!("Found {} tracks:", tracks.len());
    for track in &tracks[..] {
        println!("  - {}", track.title);
    }
    println!();

    // Step 6: Demonstrate caching functionality
    println!("Step 6: Creating a cached version of the API...");
    let cached_api = CachedMusicApi::new(SimpleMusicApi::new()).with_cascade_delete(true);
    println!("Cached API created with cascade delete enabled\n");

    // First fetch - will call the underlying API
    println!("First fetch of artist ID 1 (calls underlying API)...");
    let artist1 = cached_api.artist(&Id::Number(1)).await?;
    println!("Artist: {:?}\n", artist1.as_ref().map(|a| &a.title));

    // Second fetch - will use cache
    println!("Second fetch of artist ID 1 (uses cache)...");
    let artist_cached = cached_api.artist(&Id::Number(1)).await?;
    println!("Artist: {:?}\n", artist_cached.as_ref().map(|a| &a.title));

    // Step 7: Working with MusicApis collection
    println!("Step 7: Creating a MusicApis collection...");
    let mut apis = MusicApis::new();
    apis.add_source(Arc::new(Box::new(SimpleMusicApi::new())));
    println!("Added SimpleMusicApi to the collection\n");

    // Iterate over all APIs in the collection
    println!("Iterating over all APIs in the collection:");
    for api in &apis {
        println!("  - API source: {:?}", api.source());
    }
    println!();

    // Step 8: Demonstrate error handling
    println!("Step 8: Demonstrating error handling...");
    let non_existent_artist = api.artist(&Id::Number(999)).await?;
    match non_existent_artist {
        Some(artist) => println!("Found artist: {}", artist.title),
        None => println!("Artist with ID 999 not found (expected)\n"),
    }

    println!("=== Example completed successfully ===");
    Ok(())
}
