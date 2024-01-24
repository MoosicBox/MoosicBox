use audiotags::{AudioTag, Tag};
use futures::Future;
use lofty::{AudioFile, ParseOptions};
use moosicbox_core::{
    app::Db,
    sqlite::{db::DbError, models::TrackSource},
    types::AudioFormat,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    fs::{self, DirEntry, Metadata},
    io::Write,
    num::ParseIntError,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::output::{ScanOutput, UpdateDatabaseError};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Tag(#[from] audiotags::error::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
}

pub async fn scan(directory: &str, db: &Db, token: CancellationToken) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));
    scan_dir(
        Path::new(directory).to_path_buf(),
        output.clone(),
        token,
        Arc::new(Box::new(|a, b, c| Box::pin(scan_track(a, b, c)))),
        Some(10),
    )
    .await?;
    let end = std::time::SystemTime::now();
    log::info!(
        "Finished initial scan in {}ms",
        end.duration_since(start).unwrap().as_millis()
    );

    {
        let output = output.read().await;
        output.update_database(db).await?;
        output.reindex_global_search_index(db)?;
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
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
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
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

        let extension = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("")
            .to_uppercase();

        let format = match extension.as_str() {
            #[cfg(feature = "aac")]
            "M4A" => AudioFormat::Aac,
            #[cfg(feature = "flac")]
            "FLAC" => AudioFormat::Flac,
            #[cfg(feature = "mp3")]
            "MP3" => AudioFormat::Mp3,
            #[cfg(feature = "opus")]
            "OPUS" => AudioFormat::Opus,
            _ => AudioFormat::Source,
        };
        let bytes = metadata.len();
        let title = tag.title().unwrap_or("(untitled)").to_string();
        let number = tag.track_number().unwrap_or(1) as i32;
        let album = tag.album_title().unwrap_or("(none)").to_string();
        let artist_name = tag
            .artist()
            .or(tag.album_artist())
            .unwrap_or("(none)")
            .to_string();
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
        log::debug!("format: {:?}", format);
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

        let mut output = output.write().await;

        let count = output
            .count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;

        log::info!("Scanning track {count}");

        let artist = output.add_artist(&album_artist, &None, &None).await;
        let mut artist = artist.write().await;
        let album = artist
            .add_album(
                &album,
                &date_released,
                path_album.to_str().unwrap(),
                &None,
                &None,
            )
            .await;
        let mut album = album.write().await;

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
                let cover = Some(cover.to_str().unwrap().to_string());

                log::debug!(
                    "Found artist cover for {}: {:?}",
                    path_album.to_str().unwrap(),
                    cover
                );

                artist.cover = cover;
            }
        }

        let _ = album
            .add_track(
                &Some(path.to_str().unwrap()),
                number as u32,
                &title,
                duration,
                bytes,
                format,
                bit_depth,
                audio_bitrate,
                overall_bitrate,
                sample_rate,
                channels,
                TrackSource::Local,
                &None,
                &None,
            )
            .await;

        Ok(())
    })
}

static MUSIC_FILE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r".+\.(flac|m4a|mp3)").unwrap());
static MULTI_ARTIST_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\S,\S").unwrap());

fn process_dir_entry<F>(
    p: DirEntry,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: Arc<Box<dyn Fn(PathBuf, Arc<RwLock<ScanOutput>>, Metadata) -> Pin<Box<F>> + Send + Sync>>,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>>
where
    F: Future<Output = Result<(), ScanError>> + Send + 'static,
{
    Box::pin(async move {
        let metadata = p.metadata().unwrap();

        if metadata.is_dir() {
            scan_dir(p.path(), output.clone(), token.clone(), fun, None).await?;
        } else if metadata.is_file()
            && MUSIC_FILE_PATTERN.is_match(p.path().file_name().unwrap().to_str().unwrap())
        {
            (fun)(p.path(), output.clone(), metadata).await?;
        }

        Ok(())
    })
}

fn scan_dir<F>(
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: Arc<Box<dyn Fn(PathBuf, Arc<RwLock<ScanOutput>>, Metadata) -> Pin<Box<F>> + Send + Sync>>,
    max_parallel: Option<u8>,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>>
where
    F: Future<Output = Result<(), ScanError>> + Send + 'static,
{
    Box::pin(async move {
        let dir = match fs::read_dir(path) {
            Ok(dir) => dir,
            Err(_err) => return Ok(()),
        };

        if let Some(max_parallel) = max_parallel {
            let mut chunks = vec![];

            for (c, p) in dir.filter_map(|p| p.ok()).enumerate() {
                if chunks.len() < (max_parallel as usize) {
                    chunks.push(vec![p]);
                } else {
                    chunks[c % (max_parallel as usize)].push(p);
                }
            }

            let mut handles = chunks
                .into_iter()
                .map(move |batch| {
                    let output = output.clone();
                    let token = token.clone();
                    let fun = fun.clone();
                    std::thread::spawn(|| async move {
                        for p in batch {
                            if token.is_cancelled() {
                                break;
                            }
                            process_dir_entry(p, output.clone(), token.clone(), fun.clone())
                                .await
                                .unwrap();
                        }
                    })
                })
                .collect::<Vec<_>>();

            while let Some(cur_thread) = handles.pop() {
                cur_thread.join().unwrap().await;
            }
        } else {
            for p in dir.filter_map(|p| p.ok()) {
                if token.is_cancelled() {
                    break;
                }
                process_dir_entry(p, output.clone(), token.clone(), fun.clone())
                    .await
                    .unwrap();
            }
        }

        Ok(())
    })
}
