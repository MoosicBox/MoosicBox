use std::{
    collections::HashMap,
    sync::{atomic::AtomicU32, Arc, RwLock},
};

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
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ScanTrack {
    pub path: String,
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
}

impl ScanTrack {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: &str,
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
    ) -> Self {
        Self {
            path: path.to_string(),
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanAlbum {
    pub name: String,
    pub cover: Option<String>,
    pub searched_cover: bool,
    pub date_released: Option<String>,
    pub directory: String,
    pub tracks: Arc<RwLock<Vec<Arc<RwLock<ScanTrack>>>>>,
}

impl ScanAlbum {
    pub fn new(name: &str, date_released: &Option<String>, directory: &str) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            date_released: date_released.clone(),
            directory: directory.to_string(),
            tracks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_track(
        &mut self,
        path: &str,
        number: u32,
        title: &str,
        duration: f64,
        bytes: u64,
        format: AudioFormat,
        bit_depth: &Option<u8>,
        audio_bitrate: &Option<u32>,
        overall_bitrate: &Option<u32>,
        sample_rate: &Option<u32>,
        channels: &Option<u8>,
        source: TrackSource,
    ) -> Arc<RwLock<ScanTrack>> {
        if let Some(track) = {
            let tracks = self.tracks.read().unwrap_or_else(|e| e.into_inner());
            tracks
                .iter()
                .find(|entry| {
                    let t = entry.read().unwrap_or_else(|e| e.into_inner());
                    t.path == path
                })
                .cloned()
        } {
            track
        } else {
            let track = Arc::new(RwLock::new(ScanTrack::new(
                path,
                number,
                title,
                duration,
                bytes,
                format,
                bit_depth,
                audio_bitrate,
                overall_bitrate,
                sample_rate,
                channels,
                source,
            )));
            self.tracks.write().unwrap().push(track.clone());

            track
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanArtist {
    pub name: String,
    pub cover: Option<String>,
    pub searched_cover: bool,
    pub albums: Arc<RwLock<Vec<Arc<RwLock<ScanAlbum>>>>>,
}

impl ScanArtist {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            albums: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn add_album(
        &mut self,
        name: &str,
        date_released: &Option<String>,
        directory: &str,
    ) -> Arc<RwLock<ScanAlbum>> {
        if let Some(album) = {
            let albums = self.albums.read().unwrap_or_else(|e| e.into_inner());
            albums
                .iter()
                .find(|entry| {
                    let a = entry.read().unwrap_or_else(|e| e.into_inner());
                    a.name == name
                })
                .cloned()
        } {
            album
        } else {
            let album = Arc::new(RwLock::new(ScanAlbum::new(name, date_released, directory)));
            self.albums
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .push(album.clone());

            album
        }
    }
}

#[derive(Debug, Error)]
pub enum UpdateDatabaseError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Invalid data: {0}")]
    InvalidData(String),
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

    pub fn add_artist(&mut self, name: &str) -> Arc<RwLock<ScanArtist>> {
        if let Some(artist) = {
            let artists = self.artists.read().unwrap_or_else(|e| e.into_inner());
            artists
                .iter()
                .find(|entry| {
                    let a = entry.read().unwrap_or_else(|e| e.into_inner());
                    a.name == name
                })
                .cloned()
        } {
            artist
        } else {
            let artist = Arc::new(RwLock::new(ScanArtist::new(name)));
            self.artists
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .push(artist.clone());

            artist
        }
    }

    pub fn update_database(&self, db: &Db) -> Result<(), UpdateDatabaseError> {
        let artists = self
            .artists
            .read()
            .unwrap()
            .iter()
            .map(|artist| artist.read().unwrap().clone())
            .collect::<Vec<_>>();
        let artist_count = artists.len();
        let albums = artists
            .iter()
            .flat_map(|artist| {
                let artist = artist.albums.read().unwrap();
                let x = artist
                    .iter()
                    .map(|a| a.read().unwrap().clone())
                    .collect::<Vec<_>>();
                x
            })
            .collect::<Vec<_>>();
        let album_count = albums.len();
        let tracks = albums
            .iter()
            .flat_map(|album| {
                let album = album.tracks.read().unwrap();
                let x = album
                    .iter()
                    .map(|a| a.read().unwrap().clone())
                    .collect::<Vec<_>>();
                x
            })
            .collect::<Vec<_>>();
        let track_count = tracks.len();

        log::info!("Scanned {artist_count} artists, {album_count} albums, {track_count} tracks");

        let db_start = std::time::SystemTime::now();

        let library = db.library.lock().unwrap_or_else(|e| e.into_inner());

        let db_artists_start = std::time::SystemTime::now();
        let db_artists = add_artist_maps_and_get_artists(
            &library.inner,
            artists
                .iter()
                .map(|artist| {
                    HashMap::from([
                        ("title", SqliteValue::String(artist.name.clone())),
                        ("cover", SqliteValue::StringOpt(artist.cover.clone())),
                    ])
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
        let album_maps = artists
            .iter()
            .zip(db_artists.iter())
            .flat_map(|(artist, db)| {
                artist
                    .albums
                    .read()
                    .unwrap()
                    .iter()
                    .map(|album| {
                        let album = album.read().unwrap();
                        HashMap::from([
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
                        ])
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let db_albums = add_album_maps_and_get_albums(&library.inner, album_maps).unwrap();

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
        let insert_tracks = albums
            .iter()
            .zip(db_albums.iter())
            .flat_map(|(album, db)| {
                album
                    .tracks
                    .read()
                    .unwrap()
                    .iter()
                    .map(|track| {
                        let track = track.read().unwrap();
                        InsertTrack {
                            album_id: db.id,
                            file: track.path.clone(),
                            track: Track {
                                number: track.number as i32,
                                title: track.name.clone(),
                                duration: track.duration,
                                format: Some(track.format),
                                source: track.source,
                                ..Default::default()
                            },
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let db_tracks = add_tracks(&library.inner, insert_tracks).unwrap();

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

        set_track_sizes(&library.inner, &track_sizes).unwrap();

        let db_track_sizes_end = std::time::SystemTime::now();
        log::info!(
            "Finished db track_sizes update for scan in {}ms",
            db_track_sizes_end
                .duration_since(db_track_sizes_start)
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
