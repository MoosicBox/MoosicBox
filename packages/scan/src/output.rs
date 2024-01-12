use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{atomic::AtomicU32, Arc},
};

use futures::future::join_all;
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{
            add_album_maps_and_get_albums, add_artist_maps_and_get_artists, add_tracks,
            set_track_sizes, DbError, InsertTrack, SetTrackSize, SqliteValue,
        },
        models::{Track, TrackSource},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_search::data::ReindexFromDbError;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::CACHE_DIR;

static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

static NON_ALPHA_NUMERIC_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

pub fn sanitize_filename(string: &str) -> String {
    NON_ALPHA_NUMERIC_REGEX.replace_all(string, "_").to_string()
}

fn save_bytes_to_file(bytes: &[u8], path: &PathBuf) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    let _ = file.write_all(bytes);
}

#[derive(Debug, Error)]
pub enum FetchInternetImgError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

async fn fetch_internet_img(
    client: &reqwest::Client,
    path: &Path,
    name: &str,
    url: &str,
) -> Result<PathBuf, FetchInternetImgError> {
    let bytes = client.get(url).send().await?.bytes().await?;
    let cover_file_path = path.join(name);
    save_bytes_to_file(&bytes, &cover_file_path);
    Ok(cover_file_path)
}

async fn search_for_cover(
    client: &reqwest::Client,
    path: &Path,
    name: &str,
    url: &str,
) -> Result<Option<PathBuf>, FetchInternetImgError> {
    std::fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Failed to create config directory at {path:?}"));

    log::debug!("Searching for existing cover in {path:?}...");

    if let Some(cover_file) = std::fs::read_dir(path)
        .unwrap()
        .filter_map(|p| p.ok())
        .find(|p| p.file_name().to_str().unwrap() == name)
        .map(|dir| dir.path())
    {
        log::debug!("Found existing cover in {path:?}: '{cover_file:?}'");
        Ok(Some(cover_file))
    } else {
        log::debug!("No existing cover in {path:?}, searching internet");
        Ok(Some(fetch_internet_img(client, path, name, url).await?))
    }
}

#[derive(Debug, Clone)]
pub struct ScanTrack {
    pub path: Option<String>,
    pub number: u32,
    pub name: String,
    pub duration: f64,
    pub bytes: u64,
    pub format: AudioFormat,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackSource,
    pub tidal_id: Option<u64>,
}

impl ScanTrack {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: &Option<&str>,
        number: u32,
        name: &str,
        duration: f64,
        bytes: u64,
        format: AudioFormat,
        bit_depth: &Option<u8>,
        audio_bitrate: &Option<u32>,
        overall_bitrate: &Option<u32>,
        sample_rate: &Option<u32>,
        channels: &Option<u8>,
        source: TrackSource,
        tidal_id: &Option<u64>,
    ) -> Self {
        Self {
            path: path.map(|p| p.to_string()),
            number,
            name: name.to_string(),
            duration,
            bytes,
            format,
            bit_depth: *bit_depth,
            audio_bitrate: *audio_bitrate,
            overall_bitrate: *overall_bitrate,
            sample_rate: *sample_rate,
            channels: *channels,
            source,
            tidal_id: *tidal_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanAlbum {
    artist: ScanArtist,
    pub name: String,
    pub cover: Option<String>,
    pub searched_cover: bool,
    pub date_released: Option<String>,
    pub directory: String,
    pub tracks: Arc<RwLock<Vec<Arc<RwLock<ScanTrack>>>>>,
    pub tidal_id: Option<u64>,
}

impl ScanAlbum {
    pub fn new(
        artist: ScanArtist,
        name: &str,
        date_released: &Option<String>,
        directory: &str,
        tidal_id: &Option<u64>,
    ) -> Self {
        Self {
            artist,
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            date_released: date_released.clone(),
            directory: directory.to_string(),
            tracks: Arc::new(RwLock::new(Vec::new())),
            tidal_id: *tidal_id,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_track(
        &mut self,
        path: &Option<&str>,
        number: u32,
        name: &str,
        duration: f64,
        bytes: u64,
        format: AudioFormat,
        bit_depth: &Option<u8>,
        audio_bitrate: &Option<u32>,
        overall_bitrate: &Option<u32>,
        sample_rate: &Option<u32>,
        channels: &Option<u8>,
        source: TrackSource,
        tidal_id: &Option<u64>,
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
                tidal_id,
            )));
            self.tracks.write().await.push(track.clone());

            track
        }
    }

    #[allow(unused)]
    pub async fn search_cover(
        &mut self,
        url: String,
    ) -> Result<Option<String>, FetchInternetImgError> {
        if self.cover.is_none() && !self.searched_cover {
            let path = CACHE_DIR
                .join(sanitize_filename(&self.artist.name))
                .join(sanitize_filename(&self.name));

            let cover = search_for_cover(&IMAGE_CLIENT, &path, "album.jpg", &url).await?;

            self.searched_cover = true;

            if let Some(cover) = cover {
                self.cover = Some(cover.to_str().unwrap().to_string());
            }
        }

        Ok(self.cover.clone())
    }
}

#[derive(Debug, Clone)]
pub struct ScanArtist {
    pub name: String,
    pub cover: Option<String>,
    pub searched_cover: bool,
    pub albums: Arc<RwLock<Vec<Arc<RwLock<ScanAlbum>>>>>,
    pub tidal_id: Option<u64>,
}

impl ScanArtist {
    pub fn new(name: &str, tidal_id: &Option<u64>) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            albums: Arc::new(RwLock::new(Vec::new())),
            tidal_id: *tidal_id,
        }
    }

    pub async fn add_album(
        &mut self,
        name: &str,
        date_released: &Option<String>,
        directory: &str,
        tidal_id: &Option<u64>,
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
            maybe_entry
        } {
            album
        } else {
            let album = Arc::new(RwLock::new(ScanAlbum::new(
                self.clone(),
                name,
                date_released,
                directory,
                tidal_id,
            )));
            self.albums.write().await.push(album.clone());

            album
        }
    }

    #[allow(unused)]
    pub async fn search_cover(
        &mut self,
        url: String,
    ) -> Result<Option<String>, FetchInternetImgError> {
        if self.cover.is_none() && !self.searched_cover {
            self.searched_cover = true;

            let path = CACHE_DIR.join(sanitize_filename(&self.name));
            let cover = search_for_cover(&IMAGE_CLIENT, &path, "artist.jpg", &url).await?;

            if let Some(cover) = cover {
                self.cover = Some(cover.to_str().unwrap().to_string());
            }
        }

        Ok(self.cover.clone())
    }
}

#[derive(Debug, Error)]
pub enum UpdateDatabaseError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error(transparent)]
    ReindexFromDb(#[from] ReindexFromDbError),
}

#[derive(Clone)]
pub struct ScanOutput {
    pub artists: Arc<RwLock<Vec<Arc<RwLock<ScanArtist>>>>>,
    pub count: Arc<AtomicU32>,
}

impl ScanOutput {
    pub fn new() -> Self {
        Self {
            artists: Arc::new(RwLock::new(Vec::new())),
            count: Arc::new(AtomicU32::new(0)),
        }
    }

    pub async fn add_artist(
        &mut self,
        name: &str,
        tidal_id: &Option<u64>,
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
            maybe_entry
        } {
            artist
        } else {
            let artist = Arc::new(RwLock::new(ScanArtist::new(name, tidal_id)));
            self.artists.write().await.push(artist.clone());

            artist
        }
    }

    pub async fn update_database(&self, db: &Db) -> Result<(), UpdateDatabaseError> {
        let artists = join_all(
            self.artists
                .read()
                .await
                .iter()
                .map(|artist| async { artist.read().await.clone() })
                .collect::<Vec<_>>(),
        )
        .await;
        let artist_count = artists.len();
        let albums = join_all(artists.iter().map(|artist| async {
            let artist = artist.albums.read().await;
            join_all(
                artist
                    .iter()
                    .map(|a| async { a.read().await.clone() })
                    .collect::<Vec<_>>(),
            )
            .await
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let album_count = albums.len();
        let tracks = join_all(albums.iter().map(|album| async {
            let tracks = album.tracks.read().await;
            join_all(
                tracks
                    .iter()
                    .map(|a| async { a.read().await.clone() })
                    .collect::<Vec<_>>(),
            )
            .await
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let track_count = tracks.len();

        log::info!("Scanned {artist_count} artists, {album_count} albums, {track_count} tracks");

        let db_start = std::time::SystemTime::now();

        let db_artists_start = std::time::SystemTime::now();

        let db_artists = add_artist_maps_and_get_artists(
            &db.library
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .inner,
            artists
                .iter()
                .map(|artist| {
                    let mut values = HashMap::from([
                        ("title", SqliteValue::String(artist.name.clone())),
                        ("cover", SqliteValue::StringOpt(artist.cover.clone())),
                    ]);
                    if let Some(tidal_id) = artist.tidal_id {
                        values.insert("tidal_id", SqliteValue::Number(tidal_id as i64));
                    }
                    values
                })
                .collect(),
        )
        .unwrap();

        let db_artists_end = std::time::SystemTime::now();
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

        let db_albums_start = std::time::SystemTime::now();

        let album_maps = join_all(artists.iter().zip(db_artists.iter()).map(
            |(artist, db)| async {
                join_all(artist.albums.read().await.iter().map(|album| async {
                    let album = album.read().await;
                    let mut values = HashMap::from([
                        ("artist_id", SqliteValue::Number(db.id as i64)),
                        ("title", SqliteValue::String(album.name.clone())),
                        (
                            "date_released",
                            SqliteValue::StringOpt(album.date_released.clone()),
                        ),
                        ("artwork", SqliteValue::StringOpt(album.cover.clone())),
                        (
                            "directory",
                            SqliteValue::StringOpt(Some(album.directory.clone())),
                        ),
                    ]);
                    if let Some(tidal_id) = album.tidal_id {
                        values.insert("tidal_id", SqliteValue::Number(tidal_id as i64));
                    }
                    values
                }))
                .await
            },
        ))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let db_albums = add_album_maps_and_get_albums(
            &db.library
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .inner,
            album_maps,
        )
        .unwrap();

        let db_albums_end = std::time::SystemTime::now();
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

        let db_tracks_start = std::time::SystemTime::now();

        let insert_tracks = join_all(albums.iter().zip(db_albums.iter()).map(
            |(album, db)| async {
                join_all(album.tracks.read().await.iter().map(|track| async {
                    let track = track.read().await;
                    InsertTrack {
                        album_id: db.id,
                        file: track.path.clone(),
                        tidal_id: track.tidal_id,
                        track: Track {
                            number: track.number as i32,
                            title: track.name.clone(),
                            duration: track.duration,
                            format: Some(track.format),
                            source: track.source,
                            ..Default::default()
                        },
                    }
                }))
                .await
            },
        ))
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let db_tracks = add_tracks(
            &db.library
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .inner,
            insert_tracks,
        )
        .unwrap();

        let db_tracks_end = std::time::SystemTime::now();
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

        let db_track_sizes_start = std::time::SystemTime::now();
        let track_sizes = tracks
            .iter()
            .zip(db_tracks.iter())
            .map(|(track, db_track)| SetTrackSize {
                track_id: db_track.id,
                quality: PlaybackQuality {
                    format: track.format,
                },
                bytes: track.bytes,
                bit_depth: Some(track.bit_depth),
                audio_bitrate: Some(track.audio_bitrate),
                overall_bitrate: Some(track.overall_bitrate),
                sample_rate: Some(track.sample_rate),
                channels: Some(track.channels),
            })
            .collect::<Vec<_>>();

        set_track_sizes(
            &db.library
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .inner,
            &track_sizes,
        )
        .unwrap();

        let db_track_sizes_end = std::time::SystemTime::now();
        log::info!(
            "Finished db track_sizes update for scan in {}ms",
            db_track_sizes_end
                .duration_since(db_track_sizes_start)
                .unwrap()
                .as_millis()
        );

        let reindex_start = std::time::SystemTime::now();

        moosicbox_search::data::reindex_global_search_index_from_db(
            &db.library
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )?;

        let reindex_end = std::time::SystemTime::now();
        log::info!(
            "Finished search reindex update for scan in {}ms",
            reindex_end
                .duration_since(reindex_start)
                .unwrap()
                .as_millis()
        );

        let end = std::time::SystemTime::now();
        log::info!(
            "Finished db update for scan in {}ms",
            end.duration_since(db_start).unwrap().as_millis(),
        );

        Ok(())
    }
}
