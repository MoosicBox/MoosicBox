use audiotags::{AudioTag, Tag};
use lofty::{AudioFile, ParseOptions};
use log::info;
use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::{
            add_album_maps_and_get_albums, add_artist_maps_and_get_artists, add_tracks,
            set_track_sizes, DbError, InsertTrack, SetTrackSize, SqliteValue,
        },
        models::Track,
    },
    types::{AudioFormat, PlaybackQuality},
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashMap,
    fs::{self, DirEntry, Metadata},
    io::Write,
    num::ParseIntError,
    path::{Path, PathBuf},
    sync::{atomic::AtomicU32, Arc, RwLock},
    thread,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Tag(#[from] audiotags::error::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
struct ScanTrack {
    path: String,
    number: u32,
    name: String,
    bytes: u64,
    duration: f64,
    bit_depth: Option<u8>,
    audio_bitrate: Option<u32>,
    overall_bitrate: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u8>,
}

impl ScanTrack {
    fn new(
        path: &str,
        number: u32,
        name: &str,
        duration: f64,
        bytes: u64,
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
            bit_depth: bit_depth.clone(),
            audio_bitrate: audio_bitrate.clone(),
            overall_bitrate: overall_bitrate.clone(),
            sample_rate: sample_rate.clone(),
            channels: channels.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct ScanAlbum {
    name: String,
    cover: Option<String>,
    searched_cover: bool,
    date_released: Option<String>,
    directory: String,
    tracks: Arc<RwLock<Vec<Arc<RwLock<ScanTrack>>>>>,
}

impl ScanAlbum {
    fn new(name: &str, date_released: &Option<String>, directory: &str) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            date_released: date_released.clone(),
            directory: directory.to_string(),
            tracks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn add_track(
        &mut self,
        path: &str,
        number: u32,
        title: &str,
        duration: f64,
        bytes: u64,
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
                .map(|entry| entry.clone())
        } {
            track
        } else {
            let track = Arc::new(RwLock::new(ScanTrack::new(
                path,
                number,
                title,
                duration,
                bytes,
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
struct ScanArtist {
    name: String,
    cover: Option<String>,
    searched_cover: bool,
    albums: Arc<RwLock<Vec<Arc<RwLock<ScanAlbum>>>>>,
}

impl ScanArtist {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cover: None,
            searched_cover: false,
            albums: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn add_album(
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
                .map(|entry| entry.clone())
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
struct ScanOutput {
    artists: Arc<RwLock<Vec<Arc<RwLock<ScanArtist>>>>>,
    count: Arc<AtomicU32>,
}

impl ScanOutput {
    fn new() -> Self {
        Self {
            artists: Arc::new(RwLock::new(Vec::new())),
            count: Arc::new(AtomicU32::new(0)),
        }
    }

    fn add_artist(&mut self, name: &str) -> Arc<RwLock<ScanArtist>> {
        if let Some(artist) = {
            let artists = self.artists.read().unwrap_or_else(|e| e.into_inner());
            artists
                .iter()
                .find(|entry| {
                    let a = entry.read().unwrap_or_else(|e| e.into_inner());
                    a.name == name
                })
                .map(|entry| entry.clone())
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

pub fn scan(directory: &str, data: &AppState, token: CancellationToken) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));
    scan_dir(
        Path::new(directory).to_path_buf(),
        output.clone(),
        token,
        scan_track,
        Some(10),
    )?;
    let end = std::time::SystemTime::now();
    let output = output.read().unwrap();
    let artists = output
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
    info!(
        "Finished initial scan in {}ms {artist_count} artists, {album_count} albums, {track_count} tracks",
        end.duration_since(start).unwrap().as_millis()
    );
    let db_start = std::time::SystemTime::now();

    let library = data
        .db
        .as_ref()
        .ok_or(ScanError::NoDb)?
        .library
        .lock()
        .unwrap_or_else(|e| e.into_inner());

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
    info!(
        "Finished db artists update for scan in {}ms",
        db_artists_end
            .duration_since(db_artists_start)
            .unwrap()
            .as_millis()
    );

    if artist_count != db_artists.len() {
        return Err(ScanError::InvalidData(format!(
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
    info!(
        "Finished db albums update for scan in {}ms",
        db_albums_end
            .duration_since(db_albums_start)
            .unwrap()
            .as_millis()
    );

    if album_count != db_albums.len() {
        return Err(ScanError::InvalidData(format!(
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
                            ..Default::default()
                        },
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let db_tracks = add_tracks(&library.inner, insert_tracks).unwrap();

    let db_tracks_end = std::time::SystemTime::now();
    info!(
        "Finished db tracks update for scan in {}ms",
        db_tracks_end
            .duration_since(db_tracks_start)
            .unwrap()
            .as_millis()
    );

    if track_count != db_tracks.len() {
        return Err(ScanError::InvalidData(format!(
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
                format: AudioFormat::Source,
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
    info!(
        "Finished db track_sizes update for scan in {}ms",
        db_track_sizes_end
            .duration_since(db_track_sizes_start)
            .unwrap()
            .as_millis()
    );

    let end = std::time::SystemTime::now();
    info!(
        "Finished db update for scan in {}ms. Total scan took {}ms",
        end.duration_since(db_start).unwrap().as_millis(),
        end.duration_since(total_start).unwrap().as_millis()
    );

    Ok(())
}

fn save_bytes_to_file(bytes: &[u8], path: &PathBuf) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    let _ = file.write_all(bytes);
}

fn search_for_artwork(
    path: PathBuf,
    file_name: &str,
    tag: Option<Box<dyn AudioTag>>,
) -> Option<PathBuf> {
    if let Some(cover_file) = fs::read_dir(path.clone())
        .unwrap()
        .filter_map(|p| p.ok())
        .find(|p| {
            let name = p.file_name().to_str().unwrap().to_lowercase();
            name.starts_with(format!("{file_name}.").as_str())
        })
        .map(|dir| dir.path())
    {
        Some(cover_file)
    } else if let Some(tag) = tag {
        if let Some(tag_cover) = tag.album_cover() {
            let cover_file_path = match tag_cover.mime_type {
                audiotags::MimeType::Png => path.join(format!("{file_name}.png")),
                audiotags::MimeType::Jpeg => path.join(format!("{file_name}.jpg")),
                audiotags::MimeType::Tiff => path.join(format!("{file_name}.tiff")),
                audiotags::MimeType::Bmp => path.join(format!("{file_name}.bmp")),
                audiotags::MimeType::Gif => path.join(format!("{file_name}.gif")),
            };
            save_bytes_to_file(tag_cover.data, &cover_file_path);
            Some(cover_file_path)
        } else {
            None
        }
    } else {
        None
    }
}

fn scan_track(
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    metadata: Metadata,
) -> Result<(), ScanError> {
    let tag = Tag::new().read_from_path(path.to_str().unwrap())?;
    let lofty_tag = lofty::Probe::open(path.clone())
        .expect("ERROR: Bad path provided!")
        .options(ParseOptions::new().read_picture(false))
        .read()
        .expect("ERROR: Failed to read file!");

    let duration = if path.to_str().unwrap().ends_with(".mp3") {
        mp3_duration::from_path(path.as_path())
            .unwrap()
            .as_secs_f64()
    } else {
        tag.duration().unwrap()
    };

    let bytes = metadata.len();
    let title = tag.title().unwrap().to_string();
    let number = tag.track_number().unwrap_or(1) as i32;
    let album = tag.album_title().unwrap_or("(none)").to_string();
    let artist_name = tag.artist().or(tag.album_artist()).unwrap().to_string();
    let album_artist = tag
        .album_artist()
        .unwrap_or(artist_name.as_str())
        .to_string();
    let date_released = tag.date().map(|date| date.to_string());

    let path_artist = path.clone().parent().unwrap().parent().unwrap().to_owned();
    let artist_dir_name = path_artist
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let path_album = path.clone().parent().unwrap().to_owned();
    let album_dir_name = path_album
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let audio_bitrate = &lofty_tag.properties().audio_bitrate();
    let overall_bitrate = &lofty_tag.properties().overall_bitrate();
    let sample_rate = &lofty_tag.properties().sample_rate();
    let bit_depth = &lofty_tag.properties().bit_depth();
    let channels = &lofty_tag.properties().channels();

    log::debug!("====== {} ======", path.clone().to_str().unwrap());
    log::debug!("title: {}", title);
    log::debug!("number: {}", number);
    log::debug!("duration: {}", duration);
    log::debug!("bytes: {}", bytes);
    log::debug!("audio_bitrate: {:?}", audio_bitrate);
    log::debug!("overall_bitrate: {:?}", overall_bitrate);
    log::debug!("sample_rate: {:?}", sample_rate);
    log::debug!("bit_depth: {:?}", bit_depth);
    log::debug!("channels: {:?}", channels);
    log::debug!("album title: {}", album);
    log::debug!("artist directory name: {}", artist_dir_name);
    log::debug!("album directory name: {}", album_dir_name);
    log::debug!("artist: {}", artist_name.clone());
    log::debug!("album_artist: {}", album_artist.clone());
    log::debug!("date_released: {:?}", date_released);
    log::debug!("contains cover: {:?}", tag.album_cover().is_some());

    let album_artist = match MULTI_ARTIST_PATTERN.find(album_artist.as_str()) {
        Some(comma) => album_artist[..comma.start() + 1].to_string(),
        None => album_artist,
    };

    let mut output = output.write().unwrap_or_else(|e| e.into_inner());

    let count = output
        .count
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;

    log::info!("Scanning track {count}");

    let artist = output.add_artist(&album_artist);
    let mut artist = artist.write().unwrap_or_else(|e| e.into_inner());
    let album = artist.add_album(&album, &date_released, path_album.to_str().unwrap());
    let mut album = album.write().unwrap_or_else(|e| e.into_inner());

    if album.cover.is_none() && !album.searched_cover {
        album.searched_cover = true;
        if let Some(cover) = search_for_artwork(path_album.clone(), "cover", Some(tag)) {
            let cover = Some(cover.file_name().unwrap().to_str().unwrap().to_string());

            log::debug!(
                "Found album artwork for {}: {:?}",
                path_album.to_str().unwrap(),
                cover
            );

            album.cover = cover;
        }
    }

    if artist.cover.is_none() && !artist.searched_cover {
        artist.searched_cover = true;
        if let Some(cover) = search_for_artwork(path_album.clone(), "artist", None) {
            let cover = Some(cover.file_name().unwrap().to_str().unwrap().to_string());

            log::debug!(
                "Found artist cover for {}: {:?}",
                path_album.to_str().unwrap(),
                cover
            );

            artist.cover = cover;
        }
    }

    let _ = album.add_track(
        path.to_str().unwrap(),
        number as u32,
        &title,
        duration,
        bytes,
        bit_depth,
        audio_bitrate,
        overall_bitrate,
        sample_rate,
        channels,
    );

    Ok(())
}

static MUSIC_FILE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r".+\.(flac|m4a|mp3)").unwrap());
static MULTI_ARTIST_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\S,\S").unwrap());

type ScanTrackFn = fn(PathBuf, Arc<RwLock<ScanOutput>>, Metadata) -> Result<(), ScanError>;

fn process_dir_entry(
    p: DirEntry,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: ScanTrackFn,
) -> Result<(), ScanError> {
    let metadata = p.metadata().unwrap();

    if metadata.is_dir() {
        scan_dir(p.path(), output.clone(), token.clone(), fun, None)?;
    } else if metadata.is_file()
        && MUSIC_FILE_PATTERN.is_match(p.path().file_name().unwrap().to_str().unwrap())
    {
        fun(p.path(), output.clone(), metadata)?;
    }

    Ok(())
}

fn scan_dir(
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: ScanTrackFn,
    max_parallel: Option<u8>,
) -> Result<(), ScanError> {
    let dir = match fs::read_dir(path) {
        Ok(dir) => dir,
        Err(_err) => return Ok(()),
    };

    if let Some(max_parallel) = max_parallel {
        let mut chunks = vec![];
        let mut c = 0;

        for p in dir.filter_map(|p| p.ok()) {
            if chunks.len() < (max_parallel as usize) {
                chunks.push(vec![p]);
            } else {
                chunks[c % (max_parallel as usize)].push(p);
            }
            c += 1;
        }

        let mut handles = chunks
            .into_iter()
            .map(move |batch| {
                let output = output.clone();
                let token = token.clone();
                thread::spawn(move || {
                    for p in batch {
                        if token.is_cancelled() {
                            break;
                        }
                        process_dir_entry(p, output.clone(), token.clone(), fun).unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();

        while let Some(cur_thread) = handles.pop() {
            cur_thread.join().unwrap();
        }
    } else {
        for p in dir.filter_map(|p| p.ok()) {
            if token.is_cancelled() {
                break;
            }
            process_dir_entry(p, output.clone(), token.clone(), fun).unwrap();
        }
    }

    Ok(())
}
