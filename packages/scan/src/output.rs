use std::sync::{atomic::AtomicU32, Arc, RwLock};

use moosicbox_core::types::AudioFormat;

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
}
