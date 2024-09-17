use async_recursion::async_recursion;
use audiotags::Tag;
use futures::Future;
use lofty::{AudioFile, ParseOptions};
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Album, ApiSource, Artist, Track, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_database::Database;
use moosicbox_files::{sanitize_filename, search_for_cover};
use regex::Regex;
use std::{
    fs::Metadata,
    num::ParseIntError,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, LazyLock},
};
use thiserror::Error;
use tokio::{
    fs::{self, DirEntry},
    sync::RwLock,
    task::JoinError,
};
use tokio_util::sync::CancellationToken;

use crate::{
    output::{ScanOutput, UpdateDatabaseError},
    Scanner, CACHE_DIR,
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
    scanner: Scanner,
) -> Result<(), ScanError> {
    let items = scan_dir(
        Path::new(directory).to_path_buf(),
        token.clone(),
        scanner.clone(),
    )
    .await?;

    scan_items(items, db, token, scanner).await
}

pub async fn scan_items(
    items: Vec<ScanItem>,
    db: Arc<Box<dyn Database>>,
    _token: CancellationToken,
    scanner: Scanner,
) -> Result<(), ScanError> {
    {
        let track_count = items
            .iter()
            .filter(|x| matches!(x, ScanItem::Track { .. }))
            .count();
        let total = scanner.total.load(std::sync::atomic::Ordering::SeqCst);
        if total < track_count {
            scanner.increase_total(track_count - total).await;
        }
    }

    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let handles = items.into_iter().map({
        let output = output.clone();
        let scanner = scanner.clone();
        move |item| {
            let output = output.clone();
            let scanner = scanner.clone();
            moosicbox_task::spawn("scan: scan item", async move {
                match item {
                    ScanItem::Track { path, metadata, .. } => {
                        scan_track(path, output, metadata, scanner).await
                    }
                    ScanItem::AlbumCover {
                        path,
                        metadata,
                        album,
                    } => scan_album_cover(album, path, output, metadata, scanner).await,
                    ScanItem::ArtistCover {
                        path,
                        metadata,
                        artist,
                    } => scan_artist_cover(artist, path, output, metadata, scanner).await,
                }
            })
        }
    });

    for resp in futures::future::join_all(handles).await {
        resp??
    }

    log::info!("Finished initial scan");

    {
        let output = output.read().await;
        output.update_database(&**db).await?;
        output.reindex_global_search_index(&**db).await?;
    }

    log::info!("Finished total scan");

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
    scanner: Scanner,
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
        ) = moosicbox_task::spawn_blocking("scan: scan_track", move || {
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

        scanner.on_scanned_track().await;

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

fn scan_album_cover(
    album: Option<Album>,
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    _metadata: Metadata,
    _scanner: Scanner,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
        let mut output = output.write().await;

        if let Some(album) = album {
            if let Some(path_str) = path.to_str() {
                let artist = output
                    .add_artist(&album.artist, &None, ApiSource::Library)
                    .await;

                let output_album = artist
                    .write()
                    .await
                    .add_album(
                        &album.title,
                        &album.date_released,
                        path.parent().and_then(|x| x.to_str()),
                        &None,
                        ApiSource::Library,
                    )
                    .await;

                output_album.write().await.cover = Some(path_str.to_string());

                return Ok(());
            }
        }

        unimplemented!("scan album cover without Album info");
        // search in current directory for audio files
        // search in db for the file path?
        // if not then scan tag data
        // use album/artist to determine artist/album and if should be created or not
    })
}

fn scan_artist_cover(
    artist: Option<Artist>,
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    _metadata: Metadata,
    _scanner: Scanner,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
        let mut output = output.write().await;

        if let Some(artist) = artist {
            if let Some(path_str) = path.to_str() {
                let output_artist = output
                    .add_artist(&artist.title, &None, ApiSource::Library)
                    .await;

                output_artist.write().await.cover = Some(path_str.to_string());

                return Ok(());
            }
        }

        unimplemented!("scan artist cover without Artist info");
    })
}

static MUSIC_FILE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r".+\.(flac|m4a|mp3|opus)").unwrap());
static MULTI_ARTIST_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\S,\S").unwrap());

pub enum ScanItem {
    Track {
        path: PathBuf,
        metadata: Metadata,
        track: Option<Track>,
    },
    AlbumCover {
        path: PathBuf,
        metadata: Metadata,
        album: Option<Album>,
    },
    ArtistCover {
        path: PathBuf,
        metadata: Metadata,
        artist: Option<Artist>,
    },
}

#[allow(clippy::type_complexity)]
#[async_recursion]
async fn process_dir_entry(
    p: DirEntry,
    token: CancellationToken,
    scanner: Scanner,
) -> Result<Vec<ScanItem>, ScanError> {
    let metadata = p.metadata().await?;

    if metadata.is_dir() {
        Ok(scan_dir(p.path(), token.clone(), scanner.clone()).await?)
    } else if metadata.is_file()
        && MUSIC_FILE_PATTERN.is_match(p.path().file_name().unwrap().to_str().unwrap())
    {
        scanner.increase_total(1).await;
        Ok(vec![ScanItem::Track {
            path: p.path(),
            metadata,
            track: None,
        }])
    } else {
        Ok(vec![])
    }
}

#[allow(clippy::type_complexity)]
async fn scan_dir(
    path: PathBuf,
    token: CancellationToken,
    scanner: Scanner,
) -> Result<Vec<ScanItem>, ScanError> {
    let mut dir = match fs::read_dir(path).await {
        Ok(dir) => dir,
        Err(_err) => return Ok(vec![]),
    };

    let mut entries = vec![];

    while let Some(entry) = dir.next_entry().await? {
        entries.push(entry);
    }

    let mut handles = vec![];

    for entry in entries {
        handles.extend(process_dir_entry(entry, token.clone(), scanner.clone()).await?);
    }

    Ok(handles)
}
