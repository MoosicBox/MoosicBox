//! Data structures and database update operations for scan results.
//!
//! This module provides types for accumulating scanned music metadata
//! (artists, albums, tracks) and writing them to the database in batch operations.

#![allow(clippy::module_name_repetitions)]

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, atomic::AtomicU32},
};

use futures::future::join_all;
use moosicbox_date_utils::chrono;
use moosicbox_files::FetchAndSaveBytesFromRemoteUrlError;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{
    db::{
        self, InsertApiSource, InsertTrack, SetTrackSize, UpdateApiSource,
        add_album_maps_and_get_albums, add_api_sources, add_artist_maps_and_get_artists,
        add_tracks, set_track_sizes, update_api_sources,
    },
    models::{LibraryAlbum, LibraryArtist, LibraryTrack},
};
use moosicbox_music_api::models::ImageCoverSize;
use moosicbox_music_models::{
    Album, ApiSource, Artist, AudioFormat, PlaybackQuality, Track, TrackApiSource,
    id::{Id, TryFromIdError},
};
use moosicbox_search::{
    PopulateIndexError, RecreateIndexError, data::AsDataValues as _, populate_global_search_index,
};
use switchy_async::task::JoinError;
use switchy_database::{DatabaseError, DatabaseValue, TryFromError, profiles::LibraryDatabase};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::CACHE_DIR;

static IMAGE_CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

async fn search_for_cover(
    client: &switchy_http::Client,
    path: &Path,
    name: &str,
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<Option<PathBuf>, FetchAndSaveBytesFromRemoteUrlError> {
    std::fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Failed to create config directory at {}", path.display()));

    log::debug!("Searching for existing cover in {}...", path.display());

    let mut entries: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    if let Some(cover_file) = entries
        .into_iter()
        .find(|p| p.file_name().to_str().unwrap() == name)
        .map(|dir| dir.path())
    {
        log::debug!(
            "Found existing cover in {}: '{}'",
            path.display(),
            cover_file.display()
        );
        Ok(Some(cover_file))
    } else {
        log::debug!(
            "No existing cover in {}, searching internet",
            path.display()
        );
        Ok(Some(
            moosicbox_files::fetch_and_save_bytes_from_remote_url(
                client,
                &path.join(name),
                url,
                headers,
            )
            .await?,
        ))
    }
}

/// Represents a scanned track with metadata.
#[derive(Debug, Clone)]
pub struct ScanTrack {
    /// Filesystem path to the track file, `None` for remote tracks.
    pub path: Option<String>,
    /// Track number within the album.
    pub number: u32,
    /// Track title.
    pub name: String,
    /// Track duration in seconds.
    pub duration: f64,
    /// File size in bytes.
    pub bytes: Option<u64>,
    /// Audio format (e.g., FLAC, MP3).
    pub format: AudioFormat,
    /// Bit depth (e.g., 16, 24).
    pub bit_depth: Option<u8>,
    /// Audio bitrate in bits per second.
    pub audio_bitrate: Option<u32>,
    /// Overall bitrate in bits per second.
    pub overall_bitrate: Option<u32>,
    /// Sample rate in Hz.
    pub sample_rate: Option<u32>,
    /// Number of audio channels.
    pub channels: Option<u8>,
    /// Track source (Local or remote API).
    pub source: TrackApiSource,
    /// Remote API track identifier.
    pub id: Option<Id>,
    /// API source this track originates from.
    pub api_source: ApiSource,
}

impl ScanTrack {
    /// Creates a new scanned track with the specified metadata.
    #[allow(unused, clippy::too_many_arguments, clippy::ref_option_ref)]
    #[must_use]
    pub fn new(
        path: &Option<&str>,
        number: u32,
        name: &str,
        duration: f64,
        bytes: &Option<u64>,
        format: AudioFormat,
        bit_depth: &Option<u8>,
        audio_bitrate: &Option<u32>,
        overall_bitrate: &Option<u32>,
        sample_rate: &Option<u32>,
        channels: &Option<u8>,
        source: TrackApiSource,
        id: &Option<&Id>,
        api_source: ApiSource,
    ) -> Self {
        Self {
            path: path.map(ToString::to_string),
            number,
            name: name.to_string(),
            duration,
            bytes: *bytes,
            format,
            bit_depth: *bit_depth,
            audio_bitrate: *audio_bitrate,
            overall_bitrate: *overall_bitrate,
            sample_rate: *sample_rate,
            channels: *channels,
            source,
            id: id.cloned(),
            api_source,
        }
    }

    /// Converts this track to `SQLite` values for the `api_sources` table.
    ///
    /// Returns `None` for library sources that don't need API source mapping.
    #[must_use]
    pub fn to_api_source_sqlite_values<'a>(self) -> Option<Vec<(&'a str, DatabaseValue)>> {
        if self.api_source.is_library() {
            None
        } else {
            self.id.map(|id| {
                vec![
                    ("entity_type", "tracks".into()),
                    ("source", self.api_source.into()),
                    ("source_id", id.into()),
                ]
            })
        }
    }
}

/// Represents a scanned album with metadata and tracks.
#[derive(Debug, Clone)]
pub struct ScanAlbum {
    artist: ScanArtist,
    /// Album title.
    pub name: String,
    /// Path to the album cover image.
    pub cover: Option<String>,
    /// Whether a cover image search has been performed.
    pub searched_cover: bool,
    /// Album release date in ISO 8601 format.
    pub date_released: Option<String>,
    /// Album directory path for local albums.
    pub directory: Option<String>,
    /// Collection of tracks in this album.
    pub tracks: Arc<RwLock<Vec<Arc<RwLock<ScanTrack>>>>>,
    /// Remote API album identifier.
    pub id: Option<Id>,
    /// API source this album originates from.
    pub api_source: ApiSource,
}

impl ScanAlbum {
    /// Creates a new scanned album with the specified metadata.
    #[allow(unused, clippy::ref_option_ref)]
    #[must_use]
    pub fn new(
        artist: ScanArtist,
        name: &str,
        date_released: &Option<String>,
        directory: Option<&str>,
        id: &Option<&Id>,
        api_source: ApiSource,
    ) -> Self {
        Self {
            artist,
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            date_released: date_released.clone(),
            directory: directory.map(ToString::to_string),
            tracks: Arc::new(RwLock::new(Vec::new())),
            id: id.cloned(),
            api_source,
        }
    }

    /// Adds a track to this album or returns an existing track with the same path/number.
    #[allow(unused, clippy::too_many_arguments, clippy::ref_option_ref)]
    #[must_use]
    pub async fn add_track(
        &mut self,
        path: &Option<&str>,
        number: u32,
        name: &str,
        duration: f64,
        bytes: &Option<u64>,
        format: AudioFormat,
        bit_depth: &Option<u8>,
        audio_bitrate: &Option<u32>,
        overall_bitrate: &Option<u32>,
        sample_rate: &Option<u32>,
        channels: &Option<u8>,
        source: TrackApiSource,
        id: &Option<&Id>,
        api_source: ApiSource,
    ) -> Arc<RwLock<ScanTrack>> {
        if let Some(track) = {
            let tracks = self.tracks.read().await;
            let mut maybe_track = None;
            for entry in tracks.iter() {
                let t = entry.read().await;
                let is_match = if t.path.is_none() && path.is_none() {
                    t.number == number && t.name == name && t.source == source
                } else {
                    t.path
                        .as_ref()
                        .is_some_and(|p| path.is_some_and(|new_p| p == new_p))
                };
                if is_match {
                    maybe_track.replace(entry.clone());
                    break;
                }
            }
            drop(tracks);
            maybe_track
        } {
            track
        } else {
            let track = Arc::new(RwLock::new(ScanTrack::new(
                path,
                number,
                name,
                duration,
                bytes,
                format,
                bit_depth,
                audio_bitrate,
                overall_bitrate,
                sample_rate,
                channels,
                source,
                id,
                api_source,
            )));
            self.tracks.write().await.push(track.clone());

            track
        }
    }

    /// # Panics
    ///
    /// * If the cover path fails to be converted to a str
    ///
    /// # Errors
    ///
    /// * If the HTTP request failed
    /// * If there is an IO error
    #[allow(unused)]
    pub async fn search_cover(
        &mut self,
        url: String,
        headers: Option<&[(String, String)]>,
        api_source: &ApiSource,
    ) -> Result<Option<String>, FetchAndSaveBytesFromRemoteUrlError> {
        if self.cover.is_none() && !self.searched_cover {
            let path = CACHE_DIR
                .join(api_source.to_string())
                .join(moosicbox_files::sanitize_filename(&self.artist.name))
                .join(moosicbox_files::sanitize_filename(&self.name));

            let filename = if api_source.is_library() {
                "album.jpg".to_string()
            } else if let Some(id) = &self.id {
                let size = ImageCoverSize::Max;
                format!("album_{id}_{size}.jpg")
            } else {
                "album.jpg".to_string()
            };

            let cover = search_for_cover(&IMAGE_CLIENT, &path, &filename, &url, headers).await?;

            self.searched_cover = true;

            if let Some(cover) = cover {
                self.cover = Some(cover.to_str().unwrap().to_string());
            }
        }

        Ok(self.cover.clone())
    }

    /// Converts this album to `SQLite` values for the `albums` table.
    ///
    /// # Panics
    ///
    /// * If failed to convert `artist_id` to a `i64`
    #[must_use]
    pub fn to_sqlite_values<'a>(self, artist_id: u64) -> Vec<(&'a str, DatabaseValue)> {
        vec![
            (
                "artist_id",
                DatabaseValue::Int64(i64::try_from(artist_id).unwrap()),
            ),
            ("title", DatabaseValue::String(self.name)),
            (
                "date_released",
                DatabaseValue::StringOpt(self.date_released),
            ),
            ("artwork", DatabaseValue::StringOpt(self.cover)),
            ("directory", DatabaseValue::StringOpt(self.directory)),
        ]
    }

    /// Converts this album to `SQLite` values for the `api_sources` table.
    ///
    /// Returns `None` for library sources that don't need API source mapping.
    #[must_use]
    pub fn to_api_source_sqlite_values<'a>(self) -> Option<Vec<(&'a str, DatabaseValue)>> {
        if self.api_source.is_library() {
            None
        } else {
            self.id.map(|id| {
                vec![
                    ("entity_type", "albums".into()),
                    ("source", self.api_source.into()),
                    ("source_id", id.into()),
                ]
            })
        }
    }
}

/// Represents a scanned artist with metadata and albums.
#[derive(Debug, Clone)]
pub struct ScanArtist {
    /// Artist name.
    pub name: String,
    /// Path to the artist cover image.
    pub cover: Option<String>,
    /// Whether a cover image search has been performed.
    pub searched_cover: bool,
    /// Collection of albums by this artist.
    pub albums: Arc<RwLock<Vec<Arc<RwLock<ScanAlbum>>>>>,
    /// Remote API artist identifier.
    pub id: Option<Id>,
    /// API source this artist originates from.
    pub api_source: ApiSource,
}

impl ScanArtist {
    /// Creates a new scanned artist with the specified metadata.
    #[allow(unused, clippy::ref_option_ref)]
    #[must_use]
    pub fn new(name: &str, id: &Option<&Id>, api_source: ApiSource) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            albums: Arc::new(RwLock::new(Vec::new())),
            id: id.cloned(),
            api_source,
        }
    }

    /// Adds an album to this artist or returns an existing album with the same name.
    #[allow(unused, clippy::ref_option_ref)]
    pub async fn add_album(
        &mut self,
        name: &str,
        date_released: &Option<String>,
        directory: Option<&str>,
        id: &Option<&Id>,
        api_source: ApiSource,
    ) -> Arc<RwLock<ScanAlbum>> {
        if let Some(album) = {
            let albums = self.albums.read().await;
            let mut maybe_entry = None;
            for entry in albums.iter() {
                let a = entry.read().await;
                if a.name == name {
                    maybe_entry.replace(entry.clone());
                    break;
                }
            }
            drop(albums);
            maybe_entry
        } {
            album
        } else {
            let album = Arc::new(RwLock::new(ScanAlbum::new(
                self.clone(),
                name,
                date_released,
                directory,
                id,
                api_source,
            )));
            self.albums.write().await.push(album.clone());

            album
        }
    }

    /// # Panics
    ///
    /// * If the cover path fails to be converted to a str
    ///
    /// # Errors
    ///
    /// * If the HTTP request failed
    /// * If there is an IO error
    #[allow(unused)]
    pub async fn search_cover(
        &mut self,
        url: String,
        headers: Option<&[(String, String)]>,
        api_source: &ApiSource,
    ) -> Result<Option<String>, FetchAndSaveBytesFromRemoteUrlError> {
        if self.cover.is_none() && !self.searched_cover {
            self.searched_cover = true;

            let path = CACHE_DIR
                .join(api_source.to_string())
                .join(moosicbox_files::sanitize_filename(&self.name));

            let filename = if api_source.is_library() {
                "artist.jpg".to_string()
            } else if let Some(id) = &self.id {
                let size = ImageCoverSize::Max;
                format!("artist_{id}_{size}.jpg")
            } else {
                "artist.jpg".to_string()
            };

            let cover = search_for_cover(&IMAGE_CLIENT, &path, &filename, &url, headers).await?;

            if let Some(cover) = cover {
                self.cover = Some(cover.to_str().unwrap().to_string());
            }
        }

        Ok(self.cover.clone())
    }

    /// Converts this artist to `SQLite` values for the `artists` table.
    #[must_use]
    pub fn to_sqlite_values<'a>(self) -> Vec<(&'a str, DatabaseValue)> {
        vec![
            ("title", DatabaseValue::String(self.name.clone())),
            ("cover", DatabaseValue::StringOpt(self.cover)),
        ]
    }

    /// Converts this artist to `SQLite` values for the `api_sources` table.
    ///
    /// Returns `None` for library sources that don't need API source mapping.
    #[must_use]
    pub fn to_api_source_sqlite_values<'a>(self) -> Option<Vec<(&'a str, DatabaseValue)>> {
        if self.api_source.is_library() {
            None
        } else {
            self.id.map(|id| {
                vec![
                    ("entity_type", "artists".into()),
                    ("source", self.api_source.into()),
                    ("source_id", id.into()),
                ]
            })
        }
    }
}

/// Results from updating the database with scanned items.
///
/// Contains only the newly added items (items not previously in the database).
pub struct UpdateDatabaseResults {
    /// Newly added artists.
    pub artists: Vec<LibraryArtist>,
    /// Newly added albums.
    pub albums: Vec<LibraryAlbum>,
    /// Newly added tracks.
    pub tracks: Vec<LibraryTrack>,
}

/// Errors that can occur when updating the database with scan results.
#[derive(Debug, Error)]
pub enum UpdateDatabaseError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Database operation failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Type conversion failed.
    #[error(transparent)]
    TryFrom(#[from] TryFromError),
    /// Invalid data encountered during update.
    #[error("Invalid data: {0}")]
    InvalidData(String),
    /// Failed to populate search index.
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    /// Failed to recreate search index.
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    /// Failed to join asynchronous task.
    #[error(transparent)]
    Join(#[from] JoinError),
    /// Failed to convert ID type.
    #[error(transparent)]
    TryFromId(#[from] TryFromIdError),
    /// Failed to parse date/time.
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
}

/// Accumulates scanned items before writing to the database.
#[derive(Clone)]
pub struct ScanOutput {
    /// Collection of scanned artists with their albums and tracks.
    pub artists: Arc<RwLock<Vec<Arc<RwLock<ScanArtist>>>>>,
    /// Counter tracking the total number of items scanned.
    pub count: Arc<AtomicU32>,
}

impl ScanOutput {
    /// Creates a new empty scan output accumulator.
    #[allow(unused)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            artists: Arc::new(RwLock::new(Vec::new())),
            count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Adds an artist to the scan output or returns an existing artist with the same name.
    #[allow(unused, clippy::ref_option_ref)]
    pub async fn add_artist(
        &mut self,
        name: &str,
        id: &Option<&Id>,
        api_source: ApiSource,
    ) -> Arc<RwLock<ScanArtist>> {
        if let Some(artist) = {
            let artists = self.artists.read().await;
            let mut maybe_entry = None;
            for entry in artists.iter() {
                let a = entry.read().await;
                if a.name == name {
                    maybe_entry.replace(entry.clone());
                    break;
                }
            }
            drop(artists);
            maybe_entry
        } {
            artist
        } else {
            let artist = Arc::new(RwLock::new(ScanArtist::new(name, id, api_source)));
            self.artists.write().await.push(artist.clone());

            artist
        }
    }

    /// # Panics
    ///
    /// * If the ID failed to be retrieved from the row
    ///
    /// # Errors
    ///
    /// * If the database fails to update
    #[allow(unused, clippy::too_many_lines)]
    pub async fn update_database(
        &self,
        db: &LibraryDatabase,
    ) -> Result<UpdateDatabaseResults, UpdateDatabaseError> {
        let artists = join_all(
            self.artists
                .read()
                .await
                .iter()
                .map(|artist| async { artist.read().await.clone() }),
        )
        .await;
        let artist_count = artists.len();
        let albums = join_all(artists.iter().map(|artist| async {
            let artist = artist.albums.read().await;
            join_all(artist.iter().map(|a| async { a.read().await.clone() })).await
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let album_count = albums.len();
        let tracks = join_all(albums.iter().map(|album| async {
            let tracks = album.tracks.read().await;
            join_all(tracks.iter().map(|a| async { a.read().await.clone() })).await
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let track_count = tracks.len();

        log::info!("Scanned {artist_count} artists, {album_count} albums, {track_count} tracks");

        let db_start = switchy_time::now();

        let db_artists_start = switchy_time::now();

        let existing_artist_ids = db
            .select("artists")
            .columns(&["id"])
            .execute(&**db)
            .await?
            .iter()
            .map(|id| id.id().unwrap().try_into())
            .collect::<Result<BTreeSet<u64>, _>>()?;

        let db_artists = add_artist_maps_and_get_artists(
            db,
            artists
                .iter()
                .map(|artist| artist.clone().to_sqlite_values())
                .collect::<Vec<_>>(),
        )
        .await?;

        let db_artists_end = switchy_time::now();
        log::info!(
            "Finished db artists update for scan in {}ms",
            db_artists_end
                .duration_since(db_artists_start)
                .unwrap()
                .as_millis()
        );

        if artist_count != db_artists.len() {
            return Err(UpdateDatabaseError::InvalidData(format!(
                "Expected {} artists, but received {}",
                artist_count,
                db_artists.len()
            )));
        }

        let db_albums_start = switchy_time::now();

        let existing_album_ids = db
            .select("albums")
            .columns(&["id"])
            .execute(&**db)
            .await?
            .iter()
            .map(|id| id.id().unwrap().try_into())
            .collect::<Result<BTreeSet<u64>, _>>()?;

        let album_maps = join_all(artists.iter().zip(db_artists.iter()).map(
            |(artist, db)| async {
                join_all(artist.albums.read().await.iter().map(|album| async {
                    let album = album.read().await;
                    album.clone().to_sqlite_values(db.id)
                }))
                .await
            },
        ))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let db_albums = add_album_maps_and_get_albums(db, album_maps).await?;

        let db_albums_end = switchy_time::now();
        log::info!(
            "Finished db albums update for scan in {}ms",
            db_albums_end
                .duration_since(db_albums_start)
                .unwrap()
                .as_millis()
        );

        if album_count != db_albums.len() {
            return Err(UpdateDatabaseError::InvalidData(format!(
                "Expected {} albums, but received {}",
                album_count,
                db_albums.len()
            )));
        }

        let db_tracks_start = switchy_time::now();

        let existing_track_ids = db
            .select("tracks")
            .columns(&["id"])
            .execute(&**db)
            .await?
            .iter()
            .map(|id| id.id().unwrap().try_into())
            .collect::<Result<BTreeSet<u64>, _>>()?;

        let insert_tracks = join_all(albums.iter().zip(db_albums.iter()).map(
            |(album, db)| async {
                join_all(album.tracks.read().await.iter().map(|track| async {
                    let track = track.read().await;
                    Ok::<_, TryFromIdError>(InsertTrack {
                        album_id: db.id,
                        file: track.path.clone(),
                        track: LibraryTrack {
                            number: track.number,
                            title: track.name.clone(),
                            duration: track.duration,
                            format: Some(track.format),
                            source: track.source.clone(),
                            ..Default::default()
                        },
                    })
                }))
                .await
            },
        ))
        .await
        .into_iter()
        .flatten()
        .collect::<Result<Vec<_>, _>>()?;

        let db_tracks = add_tracks(db, insert_tracks).await?;

        let db_tracks_end = switchy_time::now();
        log::info!(
            "Finished db tracks update for scan in {}ms",
            db_tracks_end
                .duration_since(db_tracks_start)
                .unwrap()
                .as_millis()
        );

        if track_count != db_tracks.len() {
            return Err(UpdateDatabaseError::InvalidData(format!(
                "Expected {} tracks, but received {}",
                track_count,
                db_tracks.len()
            )));
        }

        let db_api_sources_start = switchy_time::now();

        let insert_api_sources = artists
            .iter()
            .zip(db_artists.iter())
            .filter_map(|(artist, db)| {
                artist.id.as_ref().map(|id| InsertApiSource {
                    entity_type: "artists".to_string(),
                    entity_id: db.id,
                    source: artist.api_source.to_string(),
                    source_id: id.to_string(),
                })
            })
            .chain(
                albums
                    .iter()
                    .zip(db_albums.iter())
                    .filter_map(|(album, db)| {
                        album.id.as_ref().map(|id| InsertApiSource {
                            entity_type: "albums".to_string(),
                            entity_id: db.id,
                            source: album.api_source.to_string(),
                            source_id: id.to_string(),
                        })
                    }),
            )
            .chain(
                tracks
                    .iter()
                    .zip(db_tracks.iter())
                    .filter_map(|(track, db)| {
                        track.id.as_ref().map(|id| InsertApiSource {
                            entity_type: "tracks".to_string(),
                            entity_id: db.id,
                            source: track.api_source.to_string(),
                            source_id: id.to_string(),
                        })
                    }),
            )
            .collect::<Vec<_>>();

        let db_api_sources = add_api_sources(db, insert_api_sources).await?;

        let db_api_sources_end = switchy_time::now();
        log::info!(
            "Finished {} db api_sources update for scan in {}ms",
            db_api_sources.len(),
            db_api_sources_end
                .duration_since(db_api_sources_start)
                .unwrap()
                .as_millis()
        );

        let db_api_sources_column_start = switchy_time::now();

        let insert_track_api_sources = db_api_sources
            .iter()
            .filter(|api_source| api_source.entity_type == "tracks")
            .map(|api_source| UpdateApiSource {
                entity_id: api_source.entity_id,
                source: api_source.source.clone(),
                source_id: api_source.source_id.clone(),
            })
            .collect::<Vec<_>>();

        futures::future::join_all([
            update_api_sources(db, "artists"),
            update_api_sources(db, "albums"),
            update_api_sources(db, "tracks"),
        ])
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        let db_api_sources_column_end = switchy_time::now();
        log::info!(
            "Finished db api_sources columns update for scan in {}ms",
            db_api_sources_column_end
                .duration_since(db_api_sources_column_start)
                .unwrap()
                .as_millis()
        );

        let db_track_sizes_start = switchy_time::now();
        let track_sizes = tracks
            .iter()
            .zip(db_tracks.iter())
            .map(|(track, db_track)| SetTrackSize {
                track_id: db_track.id,
                quality: PlaybackQuality {
                    format: track.format,
                },
                bytes: Some(track.bytes),
                bit_depth: Some(track.bit_depth),
                audio_bitrate: Some(track.audio_bitrate),
                overall_bitrate: Some(track.overall_bitrate),
                sample_rate: Some(track.sample_rate),
                channels: Some(track.channels),
            })
            .collect::<Vec<_>>();

        set_track_sizes(db, &track_sizes).await?;

        let db_track_sizes_end = switchy_time::now();
        log::info!(
            "Finished db track_sizes update for scan in {}ms",
            db_track_sizes_end
                .duration_since(db_track_sizes_start)
                .unwrap()
                .as_millis()
        );

        let end = switchy_time::now();
        log::info!(
            "Finished db update for scan in {}ms",
            end.duration_since(db_start).unwrap().as_millis(),
        );

        Ok(UpdateDatabaseResults {
            artists: db_artists
                .into_iter()
                .filter(|artist| !existing_artist_ids.contains(&artist.id))
                .collect::<Vec<_>>(),
            albums: db_albums
                .into_iter()
                .filter(|album| !existing_album_ids.contains(&album.id))
                .collect::<Vec<_>>(),
            tracks: db_tracks
                .into_iter()
                .filter(|track| !existing_track_ids.contains(&track.id))
                .collect::<Vec<_>>(),
        })
    }

    /// # Panics
    ///
    /// * If time went backwards
    ///
    /// # Errors
    ///
    /// * If the reindex failed
    pub async fn reindex_global_search_index(
        &self,
        db: &LibraryDatabase,
    ) -> Result<(), UpdateDatabaseError> {
        let reindex_start = switchy_time::now();

        moosicbox_search::data::recreate_global_search_index().await?;

        let artists = db::get_artists(db)
            .await?
            .into_iter()
            .map(Into::into)
            .map(|artist: Artist| artist.as_data_values())
            .collect::<Vec<_>>();

        populate_global_search_index(&artists, false).await?;

        let albums = db::get_albums(db)
            .await?
            .into_iter()
            .map(TryInto::try_into)
            .map(|album: Result<Album, _>| album.map(|x| x.as_data_values()))
            .collect::<Result<Vec<_>, _>>()?;

        populate_global_search_index(&albums, false).await?;

        let tracks = db::get_tracks(db, None)
            .await?
            .into_iter()
            .map(Into::into)
            .map(|track: Track| track.as_data_values())
            .collect::<Vec<_>>();

        populate_global_search_index(&tracks, false).await?;

        let reindex_end = switchy_time::now();
        log::info!(
            "Finished search reindex update for scan in {}ms",
            reindex_end
                .duration_since(reindex_start)
                .unwrap()
                .as_millis()
        );

        Ok(())
    }
}

impl Default for ScanOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use moosicbox_json_utils::ToValueType;
    use moosicbox_library::models::LibraryAlbumType;
    use moosicbox_music_models::{ApiSources, id::ApiId};
    use moosicbox_schema::get_sqlite_library_migrations;
    use pretty_assertions::assert_eq;
    use switchy_schema_test_utils::MigrationTestBuilder;

    use super::*;

    macro_rules! test_update_api_sources {
        ($db:ident, $init:expr $(,)?) => {
            paste::paste! {
                #[test_log::test(switchy_async::test)]
                async fn [< test_update_api_sources_ $db>]() {
                    let tidal = ApiSource::register("Tidal", "Tidal");
                    let qobuz = ApiSource::register("Qobuz", "Qobuz");

                    let db = $init;
                    let db = LibraryDatabase {
                        database: Arc::new(db),
                    };

                    MigrationTestBuilder::new(get_sqlite_library_migrations().await.expect("Failed to get migrations"))
                        .with_table_name("__moosicbox_schema_migrations")
                        .with_data_before("2025-06-03-211603_cache_api_sources_on_tables", |db| Box::pin(async move {
                            db.exec_raw(
                                "
                                INSERT INTO artists (id, title, cover) VALUES
                                    (1, 'title1', ''),
                                    (2, 'title2', ''),
                                    (3, 'title3', ''),
                                    (4, 'title4', '');
                                INSERT INTO albums (id, artist_id, title, date_released, date_added, artwork, directory, blur) VALUES
                                    (1, 1, 'title1', '2022-01-01', '2022-01-01', '', '', 0),
                                    (2, 2, 'title2', '2022-01-01', '2022-01-01', '', '', 0),
                                    (3, 3, 'title3', '2022-01-01', '2022-01-01', '', '', 0),
                                    (4, 4, 'title4', '2022-01-01', '2022-01-01', '', '', 0);
                                INSERT INTO tracks (id, album_id, number, title, duration, file, format, source) VALUES
                                    (1, 1, 1, 'title1', 10, 'file1', 'FLAC', 'LOCAL'),
                                    (2, 2, 2, 'title2', 13, 'file2', 'FLAC', 'LOCAL'),
                                    (3, 3, 3, 'title3', 19, 'file3', 'FLAC', 'LOCAL'),
                                    (4, 4, 4, 'title4', 15, 'file4', 'FLAC', 'LOCAL'),
                                    (6, 4, 4, 'title4', 15, NULL, 'SOURCE', 'LOCAL');
                            ",
                            )
                            .await?;
                            Ok(())
                        }))
                        .run(&*db)
                        .await
                        .expect("Failed to run migrations");

                    db.exec_raw(
                        "
                        INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                            ('artists', 1, 'Tidal', 'art123'),
                            ('artists', 1, 'Qobuz', 'art456'),
                            ('artists', 2, 'Tidal', 'art789'),
                            ('artists', 3, 'Qobuz', 'art101112');
                        INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                            ('albums', 1, 'Tidal', 'alb123'),
                            ('albums', 1, 'Qobuz', 'alb456'),
                            ('albums', 2, 'Tidal', 'alb789'),
                            ('albums', 3, 'Qobuz', 'alb101112');
                        INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                            ('tracks', 1, 'Tidal', '123'),
                            ('tracks', 1, 'Qobuz', '456'),
                            ('tracks', 2, 'Tidal', '789'),
                            ('tracks', 3, 'Qobuz', '101112'),
                            ('tracks', 6, 'Tidal', '123'),
                            ('tracks', 6, 'Qobuz', '123');
                    ",
                    )
                    .await
                    .expect("Failed to insert data");

                    assert_eq!(update_api_sources(&db, "artists").await.expect("Failed to update artists api sources").len(), 4);
                    assert_eq!(update_api_sources(&db, "albums").await.expect("Failed to update albums api sources").len(), 4);
                    assert_eq!(update_api_sources(&db, "tracks").await.expect("Failed to update tracks api sources").len(), 5);

                    // Verify artists migration
                    let artists = db
                        .select("artists")
                        .columns(&["api_sources"])
                        .execute(&*db)
                        .await
                        .expect("Failed to select artists");

                    assert_eq!(artists.len(), 4);
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            artists[0].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                            .with_api_id(ApiId {
                                source: tidal.clone(),
                                id: Id::String("art123".into())
                            })
                            .with_api_id(ApiId {
                                source: qobuz.clone(),
                                id: Id::String("art456".into())
                            })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            artists[1].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: tidal.clone(),
                            id: Id::String("art789".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            artists[2].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: qobuz.clone(),
                            id: Id::String("art101112".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            artists[3].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                    );

                    // Verify albums migration
                    let albums = db
                        .select("albums")
                        .columns(&["api_sources"])
                        .execute(&*db)
                        .await
                        .expect("Failed to convert api sources");

                    assert_eq!(albums.len(), 4);
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            albums[0].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                            .with_api_id(ApiId {
                                source: tidal.clone(),
                                id: Id::String("alb123".into())
                            })
                            .with_api_id(ApiId {
                                source: qobuz.clone(),
                                id: Id::String("alb456".into())
                            })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            albums[1].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: tidal.clone(),
                            id: Id::String("alb789".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            albums[2].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: qobuz.clone(),
                            id: Id::String("alb101112".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            albums[3].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                    );

                    // Verify tracks migration
                    let tracks = db
                        .select("tracks")
                        .columns(&["id", "api_sources"])
                        .sort("id", switchy_database::query::SortDirection::Asc)
                        .execute(&*db)
                        .await
                        .expect("Failed to convert api sources");

                    assert_eq!(tracks.len(), 5);
                    assert_eq!(
                        tracks
                            .iter()
                            .filter_map(|x| x.get("id").and_then(|x| x.as_u64()))
                            .collect::<Vec<_>>(),
                        vec![1, 2, 3, 4, 6]
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            tracks[0].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                            .with_api_id(ApiId {
                                source: tidal.clone(),
                                id: Id::String("123".into())
                            })
                            .with_api_id(ApiId {
                                source: qobuz.clone(),
                                id: Id::String("456".into())
                            })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            tracks[1].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: tidal.clone(),
                            id: Id::String("789".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            tracks[2].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default().with_api_id(ApiId {
                            source: qobuz.clone(),
                            id: Id::String("101112".into())
                        })
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            tracks[3].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                    );
                    assert_eq!(
                        <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                            tracks[4].get("api_sources").expect("Failed to get api sources")
                        )
                        .expect("Failed to convert api sources"),
                        ApiSources::default()
                            .with_api_id(ApiId {
                                source: tidal.clone(),
                                id: Id::String("123".into())
                            })
                            .with_api_id(ApiId {
                                source: qobuz.clone(),
                                id: Id::String("123".into())
                            })
                    );
                }
            }
        }
    }

    test_update_api_sources!(sqlx, {
        switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap()
    });
    test_update_api_sources!(rusqlite, {
        switchy::database_connection::init_sqlite_rusqlite(None).unwrap()
    });

    #[test_log::test(switchy_async::test)]
    async fn can_scan_single_artist_with_single_album_with_single_track() {
        static API_SOURCE: LazyLock<ApiSource> =
            LazyLock::new(|| ApiSource::register("MockApi", "MockApi"));

        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let mut output = ScanOutput::new();

        let api_source = API_SOURCE.clone();

        let name = "artist1";
        let id = "1".into();

        let artist = output
            .add_artist(name, &Some(&id), api_source.clone())
            .await;

        let name = "album1";
        let id = "1".into();
        let date_released = "2022-01-01".to_string();

        let album = artist
            .write()
            .await
            .add_album(
                name,
                &Some(date_released),
                None,
                &Some(&id),
                api_source.clone(),
            )
            .await;

        let name = "track1";
        let id = "1".into();

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source.clone().into(),
                &Some(&id),
                api_source.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let artists: Vec<LibraryArtist> = db
            .select("artists")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let albums: Vec<LibraryAlbum> = db
            .select("albums")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let tracks: Vec<LibraryTrack> = db
            .select("tracks")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        assert_eq!(
            artists,
            vec![LibraryArtist {
                id: 1,
                title: "artist1".to_string(),
                api_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source.clone(), "1".into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            albums,
            vec![LibraryAlbum {
                id: 1,
                title: "album1".to_string(),
                artist_id: 1,
                date_released: Some("2022-01-01".to_string()),
                date_added: albums.first().and_then(|x| x.date_added.clone()),
                album_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source.clone(), "1".into()),
                artist_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            tracks,
            vec![LibraryTrack {
                id: 1,
                number: 1,
                title: "track1".to_string(),
                duration: 10.0,
                album_id: 1,
                format: Some(AudioFormat::Source),
                source: TrackApiSource::Api(api_source.clone()),
                api_source: ApiSource::library(),
                api_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source.clone(), "1".into()),
                ..Default::default()
            }]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn should_merge_artists_with_same_id_and_name_in_different_api_sources() {
        let api_source1 = ApiSource::register("MockApi1", "MockApi1");
        let api_source2 = ApiSource::register("MockApi2", "MockApi2");

        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let mut output = ScanOutput::new();

        let artist_name = "artist1";
        let artist_id = "1".into();

        let artist = output
            .add_artist(artist_name, &Some(&artist_id), api_source1.clone())
            .await;

        let album_name = "album1";
        let album_id = "1".into();
        let album_date_released = "2022-01-01".to_string();

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released.clone()),
                None,
                &Some(&album_id),
                api_source1.clone(),
            )
            .await;

        let track_name = "track1";
        let track_id = "1".into();

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source1.clone().into(),
                &Some(&track_id),
                api_source1.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let mut output = ScanOutput::new();

        let artist = output
            .add_artist(artist_name, &Some(&artist_id), api_source2.clone())
            .await;

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released),
                None,
                &Some(&album_id),
                api_source2.clone(),
            )
            .await;

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source2.clone().into(),
                &Some(&track_id),
                api_source2.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let artists: Vec<LibraryArtist> = db
            .select("artists")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let albums: Vec<LibraryAlbum> = db
            .select("albums")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let tracks: Vec<LibraryTrack> = db
            .select("tracks")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        assert_eq!(
            artists,
            vec![LibraryArtist {
                id: 1,
                title: "artist1".to_string(),
                api_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source1.clone(), "1".into())
                    .with_source(api_source2.clone(), "1".into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            albums,
            vec![LibraryAlbum {
                id: 1,
                title: "album1".to_string(),
                artist_id: 1,
                date_released: Some("2022-01-01".to_string()),
                date_added: albums.first().and_then(|x| x.date_added.clone()),
                album_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source1.clone(), "1".into())
                    .with_source(api_source2.clone(), "1".into()),
                artist_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            tracks,
            vec![
                LibraryTrack {
                    id: 1,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source1.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default()
                        .with_source(ApiSource::library(), 1.into())
                        .with_source(api_source1.clone(), "1".into()),
                    ..Default::default()
                },
                LibraryTrack {
                    id: 2,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source2.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default()
                        .with_source(ApiSource::library(), 2.into())
                        .with_source(api_source2.clone(), "1".into()),
                    ..Default::default()
                }
            ]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn should_merge_artists_with_different_id_and_same_name_in_different_api_sources() {
        let api_source1 = ApiSource::register("MockApi1", "MockApi1");
        let api_source2 = ApiSource::register("MockApi2", "MockApi2");

        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let mut output = ScanOutput::new();

        let artist_name = "artist1";
        let artist_id = "1".into();

        let artist = output
            .add_artist(artist_name, &Some(&artist_id), api_source1.clone())
            .await;

        let album_name = "album1";
        let album_id = "1".into();
        let album_date_released = "2022-01-01".to_string();

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released.clone()),
                None,
                &Some(&album_id),
                api_source1.clone(),
            )
            .await;

        let track_name = "track1";
        let track_id = "1".into();

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source1.clone().into(),
                &Some(&track_id),
                api_source1.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let mut output = ScanOutput::new();

        let artist_id = "10".into();

        let artist = output
            .add_artist(artist_name, &Some(&artist_id), api_source2.clone())
            .await;

        let album_id = "10".into();

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released),
                None,
                &Some(&album_id),
                api_source2.clone(),
            )
            .await;

        let track_id = "10".into();

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source2.clone().into(),
                &Some(&track_id),
                api_source2.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let artists: Vec<LibraryArtist> = db
            .select("artists")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let albums: Vec<LibraryAlbum> = db
            .select("albums")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let tracks: Vec<LibraryTrack> = db
            .select("tracks")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        assert_eq!(
            artists,
            vec![LibraryArtist {
                id: 1,
                title: "artist1".to_string(),
                api_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source1.clone(), "1".into())
                    .with_source(api_source2.clone(), "10".into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            albums,
            vec![LibraryAlbum {
                id: 1,
                title: "album1".to_string(),
                artist_id: 1,
                date_released: Some("2022-01-01".to_string()),
                date_added: albums.first().and_then(|x| x.date_added.clone()),
                album_sources: ApiSources::default()
                    .with_source(ApiSource::library(), 1.into())
                    .with_source(api_source1.clone(), "1".into())
                    .with_source(api_source2.clone(), "10".into()),
                artist_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            tracks,
            vec![
                LibraryTrack {
                    id: 1,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    album_type: LibraryAlbumType::default(),
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source1.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default()
                        .with_source(ApiSource::library(), 1.into())
                        .with_source(api_source1.clone(), "1".into()),
                    ..Default::default()
                },
                LibraryTrack {
                    id: 2,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source2.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default()
                        .with_source(ApiSource::library(), 2.into())
                        .with_source(api_source2.clone(), "10".into()),
                    ..Default::default()
                }
            ]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn should_merge_artists_with_no_id_and_same_name_in_different_api_sources() {
        let api_source1 = ApiSource::register("MockApi1", "MockApi1");
        let api_source2 = ApiSource::register("MockApi2", "MockApi2");

        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let mut output = ScanOutput::new();

        let artist_name = "artist1";

        let artist = output
            .add_artist(artist_name, &None, api_source1.clone())
            .await;

        let album_name = "album1";
        let album_date_released = "2022-01-01".to_string();

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released.clone()),
                None,
                &None,
                api_source1.clone(),
            )
            .await;

        let track_name = "track1";

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source1.clone().into(),
                &None,
                api_source1.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let mut output = ScanOutput::new();

        let artist = output
            .add_artist(artist_name, &None, api_source2.clone())
            .await;

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released),
                None,
                &None,
                api_source2.clone(),
            )
            .await;

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source2.clone().into(),
                &None,
                api_source2.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let artists: Vec<LibraryArtist> = db
            .select("artists")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let albums: Vec<LibraryAlbum> = db
            .select("albums")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let tracks: Vec<LibraryTrack> = db
            .select("tracks")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        assert_eq!(
            artists,
            vec![LibraryArtist {
                id: 1,
                title: "artist1".to_string(),
                api_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            albums,
            vec![LibraryAlbum {
                id: 1,
                title: "album1".to_string(),
                artist_id: 1,
                date_released: Some("2022-01-01".to_string()),
                date_added: albums.first().and_then(|x| x.date_added.clone()),
                album_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                artist_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            tracks,
            vec![
                LibraryTrack {
                    id: 1,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    album_type: LibraryAlbumType::default(),
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source1.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                    ..Default::default()
                },
                LibraryTrack {
                    id: 2,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source2.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 2.into()),
                    ..Default::default()
                }
            ]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn should_merge_multiple_artists_with_same_name_in_different_api_sources() {
        let api_source1 = ApiSource::register("MockApi1", "MockApi1");
        let api_source2 = ApiSource::register("MockApi2", "MockApi2");

        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let mut output = ScanOutput::new();

        let artist_name = "artist1";

        let artist = output
            .add_artist(artist_name, &None, api_source1.clone())
            .await;

        let album_name = "album1";
        let album_date_released = "2022-01-01".to_string();

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released.clone()),
                None,
                &None,
                api_source1.clone(),
            )
            .await;

        let track_name = "track1";

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source1.clone().into(),
                &None,
                api_source1.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let mut output = ScanOutput::new();

        let _artist2 = output
            .add_artist("artist2", &None, api_source2.clone())
            .await;

        let artist = output
            .add_artist(artist_name, &None, api_source2.clone())
            .await;

        let album = artist
            .write()
            .await
            .add_album(
                album_name,
                &Some(album_date_released),
                None,
                &None,
                api_source2.clone(),
            )
            .await;

        let _ = album
            .write()
            .await
            .add_track(
                &None,
                1,
                track_name,
                10.0,
                &None,
                AudioFormat::Source,
                &None,
                &None,
                &None,
                &None,
                &None,
                api_source2.clone().into(),
                &None,
                api_source2.clone(),
            )
            .await;

        output.update_database(&db).await.unwrap();

        let artists: Vec<LibraryArtist> = db
            .select("artists")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let albums: Vec<LibraryAlbum> = db
            .select("albums")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        let tracks: Vec<LibraryTrack> = db
            .select("tracks")
            .execute(&*db)
            .await
            .unwrap()
            .to_value_type()
            .unwrap();

        assert_eq!(
            artists,
            vec![
                LibraryArtist {
                    id: 1,
                    title: "artist1".to_string(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                    ..Default::default()
                },
                LibraryArtist {
                    id: 2,
                    title: "artist2".to_string(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 2.into()),
                    ..Default::default()
                }
            ]
        );

        assert_eq!(
            albums,
            vec![LibraryAlbum {
                id: 1,
                title: "album1".to_string(),
                artist_id: 1,
                date_released: Some("2022-01-01".to_string()),
                date_added: albums.first().and_then(|x| x.date_added.clone()),
                album_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                artist_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                ..Default::default()
            }]
        );

        assert_eq!(
            tracks,
            vec![
                LibraryTrack {
                    id: 1,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    album_type: LibraryAlbumType::default(),
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source1.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 1.into()),
                    ..Default::default()
                },
                LibraryTrack {
                    id: 2,
                    number: 1,
                    title: "track1".to_string(),
                    duration: 10.0,
                    album_id: 1,
                    format: Some(AudioFormat::Source),
                    source: TrackApiSource::Api(api_source2.clone()),
                    api_source: ApiSource::library(),
                    api_sources: ApiSources::default().with_source(ApiSource::library(), 2.into()),
                    ..Default::default()
                }
            ]
        );
    }
}
