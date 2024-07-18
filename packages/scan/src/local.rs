use async_recursion::async_recursion;
use audiotags::Tag;
use futures::Future;
use lofty::{AudioFile, ParseOptions};
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{ApiSource, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_database::Database;
use moosicbox_files::{sanitize_filename, search_for_cover};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    fs::Metadata,
    num::ParseIntError,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};
use thiserror::Error;
use tokio::{
    fs::{self, DirEntry},
    sync::RwLock,
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;

use crate::{
    output::{ScanOutput, UpdateDatabaseError},
    CACHE_DIR,
};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Tag(#[from] audiotags::error::Error),
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
}

pub async fn scan(
    directory: &str,
    db: Arc<Box<dyn Database>>,
    token: CancellationToken,
) -> Result<(), ScanError> {
    let total_start = std::time::SystemTime::now();
    let start = std::time::SystemTime::now();
    let output = Arc::new(RwLock::new(ScanOutput::new()));
    let handles = scan_dir(
        Path::new(directory).to_path_buf(),
        output.clone(),
        token,
        Arc::new(Box::new(|a, b, c| Box::pin(scan_track(a, b, c)))),
    )
    .await?;

    for resp in futures::future::join_all(handles).await {
        resp??
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished initial scan in {}ms",
        end.duration_since(start).unwrap().as_millis()
    );

    {
        let output = output.read().await;
        output.update_database(&**db).await?;
        output.reindex_global_search_index(&**db).await?;
    }

    let end = std::time::SystemTime::now();
    log::info!(
        "Finished total scan in {}ms",
        end.duration_since(total_start).unwrap().as_millis(),
    );

    Ok(())
}

fn extract_track_number(track_filestem: &str) -> Option<u16> {
    let numbers = track_filestem
        .chars()
        .take_while(|c| c.is_numeric())
        .collect::<Vec<_>>();
    let number = numbers
        .into_iter()
        .skip_while(|c| *c == '0')
        .collect::<String>();

    if number.is_empty() {
        None
    } else {
        number.parse::<u16>().ok()
    }
}

fn extract_track_name(track_filestem: &str) -> Option<String> {
    let name = track_filestem
        .chars()
        .skip_while(|c| c.is_numeric() || c.is_whitespace() || *c == '-' || *c == '_')
        .map(|c| if c == '_' { ' ' } else { c })
        .collect::<String>();

    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn scan_track(
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    metadata: Metadata,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
        let (
            path,
            tag,
            path_album,
            format,
            bytes,
            title,
            number,
            duration,
            album,
            album_artist,
            date_released,
            audio_bitrate,
            overall_bitrate,
            sample_rate,
            bit_depth,
            channels,
        ) = tokio::task::spawn_blocking(move || {
            let extension = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("")
                .to_uppercase();

            let tag = match extension.as_str() {
                "FLAC" | "MP4" | "M4A" | "MP3" | "WAV" => {
                    Some(Tag::new().read_from_path(path.to_str().unwrap()))
                }
                _ => None,
            };
            let lofty_tag = lofty::Probe::open(&path)
                .expect("ERROR: Bad path provided!")
                .options(ParseOptions::new().read_picture(false))
                .read()
                .expect("ERROR: Failed to read file!");

            let duration = if path.clone().to_str().unwrap().ends_with(".mp3") {
                mp3_duration::from_path(path.as_path())
                    .unwrap()
                    .as_secs_f64()
            } else {
                match tag {
                    Some(Ok(ref tag)) => tag.duration().unwrap(),
                    Some(Err(err)) => return Err(ScanError::Tag(err)),
                    None => 10.0,
                }
            };

            let tag = tag.and_then(|x| x.ok());

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

            let track_filestem = path.file_stem().unwrap().to_str().unwrap().to_string();

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
            let title = tag
                .as_ref()
                .and_then(|tag| tag.title().map(|title| title.to_string()))
                .or_else(|| extract_track_name(&track_filestem))
                .unwrap_or("(untitled)".to_string());
            let number = tag
                .as_ref()
                .and_then(|tag| tag.track_number())
                .or_else(|| extract_track_number(&track_filestem))
                .unwrap_or(1) as i32;
            let album = tag
                .as_ref()
                .and_then(|tag| tag.album_title())
                .unwrap_or(&album_dir_name)
                .to_string();
            let artist_name = tag
                .as_ref()
                .and_then(|tag| tag.artist().or(tag.album_artist()))
                .unwrap_or(&artist_dir_name)
                .to_string();
            let album_artist = tag
                .as_ref()
                .and_then(|tag| tag.album_artist())
                .unwrap_or(artist_name.as_str())
                .to_string();
            let date_released = tag
                .as_ref()
                .and_then(|tag| tag.date())
                .map(|date| date.to_string());

            let audio_bitrate = lofty_tag.properties().audio_bitrate();
            let overall_bitrate = lofty_tag.properties().overall_bitrate();
            let sample_rate = lofty_tag.properties().sample_rate();
            let bit_depth = lofty_tag.properties().bit_depth();
            let channels = lofty_tag.properties().channels();

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
            log::debug!(
                "contains cover: {:?}",
                tag.as_ref().is_some_and(|tag| tag.album_cover().is_some())
            );

            let album_artist = match MULTI_ARTIST_PATTERN.find(album_artist.as_str()) {
                Some(comma) => album_artist[..comma.start() + 1].to_string(),
                None => album_artist,
            };

            Ok::<_, ScanError>((
                path.to_path_buf(),
                tag,
                path_album,
                format,
                bytes,
                title,
                number,
                duration,
                album,
                album_artist,
                date_released,
                audio_bitrate,
                overall_bitrate,
                sample_rate,
                bit_depth,
                channels,
            ))
        })
        .await??;

        let mut output = output.write().await;

        let count = output
            .count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;

        log::info!("Scanning track {count}");

        let artist = output
            .add_artist(&album_artist, &None, ApiSource::Library)
            .await;
        let mut artist = artist.write().await;
        let album = artist
            .add_album(
                &album,
                &date_released,
                Some(path_album.to_str().unwrap()),
                &None,
                ApiSource::Library,
            )
            .await;
        let mut album = album.write().await;
        let save_path = CACHE_DIR
            .join("local")
            .join(sanitize_filename(&artist.name))
            .join(sanitize_filename(&album.name));

        if album.cover.is_none() && !album.searched_cover {
            album.searched_cover = true;
            if let Some(cover) = search_for_cover(
                path_album.clone(),
                "cover",
                Some(save_path.join("album.jpg")),
                tag,
            )
            .await?
            {
                let cover = Some(cover.to_str().unwrap().to_string());

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
            if let Some(cover) = search_for_cover(
                path_album.clone(),
                "artist",
                Some(save_path.join("artist.jpg")),
                None,
            )
            .await?
            {
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
                &Some(bytes),
                format,
                &bit_depth,
                &audio_bitrate,
                &overall_bitrate,
                &sample_rate,
                &channels,
                TrackApiSource::Local,
                &None,
                ApiSource::Library,
            )
            .await;

        Ok(())
    })
}

static MUSIC_FILE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r".+\.(flac|m4a|mp3|opus)").unwrap());
static MULTI_ARTIST_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\S,\S").unwrap());

#[allow(clippy::type_complexity)]
#[async_recursion]
async fn process_dir_entry<F>(
    p: DirEntry,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: Arc<Box<dyn Fn(PathBuf, Arc<RwLock<ScanOutput>>, Metadata) -> Pin<Box<F>> + Send + Sync>>,
) -> Result<Vec<JoinHandle<Result<(), ScanError>>>, ScanError>
where
    F: Future<Output = Result<(), ScanError>> + Send + 'static,
{
    let metadata = p.metadata().await?;

    if metadata.is_dir() {
        Ok(scan_dir(p.path(), output.clone(), token.clone(), fun).await?)
    } else if metadata.is_file()
        && MUSIC_FILE_PATTERN.is_match(p.path().file_name().unwrap().to_str().unwrap())
    {
        Ok(vec![moosicbox_task::spawn(
            "scan: Local process_dir_entry",
            async move {
                (fun)(p.path(), output.clone(), metadata).await?;
                Ok::<_, ScanError>(())
            },
        )])
    } else {
        Ok(vec![])
    }
}

#[allow(clippy::type_complexity)]
async fn scan_dir<F>(
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    token: CancellationToken,
    fun: Arc<Box<dyn Fn(PathBuf, Arc<RwLock<ScanOutput>>, Metadata) -> Pin<Box<F>> + Send + Sync>>,
) -> Result<Vec<JoinHandle<Result<(), ScanError>>>, ScanError>
where
    F: Future<Output = Result<(), ScanError>> + Send + 'static,
{
    let mut dir = match fs::read_dir(path).await {
        Ok(dir) => dir,
        Err(_err) => return Ok(vec![]),
    };

    let mut handles = vec![];

    while let Some(entry) = dir.next_entry().await? {
        tokio::select! {
            resp = process_dir_entry(entry, output.clone(), token.clone(), fun.clone()) => {
                handles.extend(resp?)
            }
            _ = token.cancelled() => {
                log::debug!("Scan cancelled");
                break;
            }
        }
    }

    Ok(handles)
}
