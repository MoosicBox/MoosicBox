//! Local filesystem scanning functionality.
//!
//! This module provides functions to scan music files from local filesystem paths,
//! extracting metadata from audio files using tag libraries and audio file parsers.

use async_recursion::async_recursion;
use futures::Future;
use moosicbox_audiotags::Tag;
use moosicbox_files::{sanitize_filename, search_for_cover};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_lofty::{AudioFile, ParseOptions};
use moosicbox_music_models::{Album, ApiSource, Artist, AudioFormat, Track, TrackApiSource};
use regex::Regex;
use std::{
    fs::Metadata,
    num::ParseIntError,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, LazyLock},
};
use switchy_async::task::JoinError;
use switchy_async::util::CancellationToken;
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;
use tokio::{
    fs::{self, DirEntry},
    sync::RwLock,
};

use crate::{
    CACHE_DIR, Scanner,
    output::{ScanOutput, UpdateDatabaseError},
};

/// Errors that can occur during local filesystem scanning.
#[derive(Debug, Error)]
pub enum ScanError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to parse integer from string.
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    /// Failed to join asynchronous task.
    #[error(transparent)]
    Join(#[from] JoinError),
    /// Audio tag parsing failed.
    #[error(transparent)]
    Tag(#[from] moosicbox_audiotags::error::Error),
    /// I/O operation failed.
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    /// Database update operation failed.
    #[error(transparent)]
    UpdateDatabase(#[from] UpdateDatabaseError),
    /// Audio file parsing failed with Lofty.
    #[error(transparent)]
    Lofty(#[from] moosicbox_lofty::LoftyError),
}

/// Scans a local filesystem directory for music files.
///
/// # Errors
///
/// * If the scan fails
pub async fn scan(
    directory: &str,
    db: &LibraryDatabase,
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

/// Processes scanned items and updates the database with extracted metadata.
///
/// # Errors
///
/// * If the scan fails
/// * If a tokio task failed to join
pub async fn scan_items(
    items: Vec<ScanItem>,
    db: &LibraryDatabase,
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
            switchy_async::runtime::Handle::current().spawn_with_name(
                "scan: scan item",
                async move {
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
                },
            )
        }
    });

    for resp in futures::future::join_all(handles).await {
        resp??;
    }

    log::info!("Finished initial scan");

    let output = output.read().await;
    output.update_database(db).await?;
    output.reindex_global_search_index(db).await?;
    drop(output);

    log::info!("Finished total scan");

    Ok(())
}

fn extract_track_number(track_filestem: &str) -> Option<u16> {
    let number = track_filestem
        .chars()
        .take_while(|c| c.is_numeric())
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

    if name.is_empty() { None } else { Some(name) }
}

#[allow(clippy::too_many_lines)]
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
        ) = switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("scan: scan_track", move || {
                log::debug!("scan_track: path={}", path.display());
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
                let moosicbox_lofty_tag = moosicbox_lofty::Probe::open(&path)
                    .expect("ERROR: Bad path provided!")
                    .options(ParseOptions::new().read_picture(false))
                    .read()
                    .map_err(|e| {
                        moosicbox_assert::die_or_error!(
                            "Failed to read lofty tags: path={path:?} ({e:?})"
                        );
                        e
                    })?;

                let duration = if std::path::Path::new(path.clone().to_str().unwrap())
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"))
                {
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

                let tag = tag.and_then(Result::ok);

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
                    .and_then(|tag| tag.title().map(ToString::to_string))
                    .or_else(|| extract_track_name(&track_filestem))
                    .unwrap_or_else(|| "(untitled)".to_string());
                let number = i32::from(
                    tag.as_ref()
                        .and_then(|tag| tag.track_number())
                        .or_else(|| extract_track_number(&track_filestem))
                        .unwrap_or(1),
                );
                let album = tag
                    .as_ref()
                    .and_then(|tag| tag.album_title())
                    .unwrap_or(&album_dir_name)
                    .to_string();
                let artist_name = tag
                    .as_ref()
                    .and_then(|tag| tag.artist().or_else(|| tag.album_artist()))
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

                let audio_bitrate = moosicbox_lofty_tag.properties().audio_bitrate();
                let overall_bitrate = moosicbox_lofty_tag.properties().overall_bitrate();
                let sample_rate = moosicbox_lofty_tag.properties().sample_rate();
                let bit_depth = moosicbox_lofty_tag.properties().bit_depth();
                let channels = moosicbox_lofty_tag.properties().channels();

                log::debug!("====== {} ======", path.clone().to_str().unwrap());
                log::debug!("title: {title}");
                log::debug!("format: {format:?}");
                log::debug!("number: {number}");
                log::debug!("duration: {duration}");
                log::debug!("bytes: {bytes}");
                log::debug!("audio_bitrate: {audio_bitrate:?}");
                log::debug!("overall_bitrate: {overall_bitrate:?}");
                log::debug!("sample_rate: {sample_rate:?}");
                log::debug!("bit_depth: {bit_depth:?}");
                log::debug!("channels: {channels:?}");
                log::debug!("album title: {album}");
                log::debug!("artist directory name: {artist_dir_name}");
                log::debug!("album directory name: {album_dir_name}");
                log::debug!("artist: {artist_name}");
                log::debug!("album_artist: {album_artist}");
                log::debug!("date_released: {date_released:?}");
                log::debug!(
                    "contains cover: {:?}",
                    tag.as_ref().is_some_and(|tag| tag.album_cover().is_some())
                );

                let album_artist = match MULTI_ARTIST_PATTERN.find(album_artist.as_str()) {
                    Some(comma) => album_artist[..=comma.start()].to_string(),
                    None => album_artist,
                };

                Ok::<_, ScanError>((
                    path.clone(),
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
            .add_artist(&album_artist, &None, ApiSource::library())
            .await;
        let mut artist = artist.write().await;
        let album = artist
            .add_album(
                &album,
                &date_released,
                Some(path_album.to_str().unwrap()),
                &None,
                ApiSource::library(),
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

        drop(artist);

        let _ = album
            .add_track(
                &Some(path.to_str().unwrap()),
                u32::try_from(number).unwrap(),
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
                ApiSource::library(),
            )
            .await;

        drop(album);
        drop(output);

        Ok(())
    })
}

#[allow(clippy::needless_pass_by_value)]
fn scan_album_cover(
    album: Option<Album>,
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    _metadata: Metadata,
    _scanner: Scanner,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
        if let Some(album) = album
            && let Some(path_str) = path.to_str()
        {
            let artist = output
                .write()
                .await
                .add_artist(&album.artist, &None, ApiSource::library())
                .await;

            let output_album = artist
                .write()
                .await
                .add_album(
                    &album.title,
                    &album.date_released.map(|x| x.and_utc().to_rfc3339()),
                    path.parent().and_then(|x| x.to_str()),
                    &None,
                    ApiSource::library(),
                )
                .await;

            output_album.write().await.cover = Some(path_str.to_string());

            return Ok(());
        }

        unimplemented!("scan album cover without Album info");
        // search in current directory for audio files
        // search in db for the file path?
        // if not then scan tag data
        // use album/artist to determine artist/album and if should be created or not
    })
}

#[allow(clippy::needless_pass_by_value)]
fn scan_artist_cover(
    artist: Option<Artist>,
    path: PathBuf,
    output: Arc<RwLock<ScanOutput>>,
    _metadata: Metadata,
    _scanner: Scanner,
) -> Pin<Box<dyn Future<Output = Result<(), ScanError>> + Send>> {
    Box::pin(async move {
        if let Some(artist) = artist
            && let Some(path_str) = path.to_str()
        {
            let output_artist = output
                .write()
                .await
                .add_artist(&artist.title, &None, ApiSource::library())
                .await;

            output_artist.write().await.cover = Some(path_str.to_string());

            return Ok(());
        }

        unimplemented!("scan artist cover without Artist info");
    })
}

static MUSIC_FILE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r".+\.(flac|m4a|mp3|opus)").unwrap());
static MULTI_ARTIST_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\S,\S").unwrap());

/// Represents an item discovered during filesystem scanning.
///
/// During directory traversal, files are classified into scan items for processing.
pub enum ScanItem {
    /// A music track file with metadata.
    Track {
        /// Path to the track file.
        path: PathBuf,
        /// File metadata (size, permissions, etc.).
        metadata: Metadata,
        /// Track information if already known.
        track: Option<Track>,
    },
    /// An album cover image file.
    AlbumCover {
        /// Path to the cover image file.
        path: PathBuf,
        /// File metadata (size, permissions, etc.).
        metadata: Metadata,
        /// Album information if already known.
        album: Option<Album>,
    },
    /// An artist cover image file.
    ArtistCover {
        /// Path to the cover image file.
        path: PathBuf,
        /// File metadata (size, permissions, etc.).
        metadata: Metadata,
        /// Artist information if already known.
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

    // Sort entries for deterministic processing
    entries.sort_by_key(tokio::fs::DirEntry::file_name);

    let mut handles = vec![];

    for entry in entries {
        handles.extend(process_dir_entry(entry, token.clone(), scanner.clone()).await?);
    }

    Ok(handles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_extract_track_number_with_leading_digits() {
        assert_eq!(extract_track_number("01 Song Title"), Some(1));
        assert_eq!(extract_track_number("05 Song Title"), Some(5));
        assert_eq!(extract_track_number("12 Song Title"), Some(12));
        assert_eq!(extract_track_number("123 Song Title"), Some(123));
    }

    #[test_log::test]
    fn test_extract_track_number_with_leading_zeros() {
        // Leading zeros should be skipped
        assert_eq!(extract_track_number("001 Song Title"), Some(1));
        assert_eq!(extract_track_number("007 Song Title"), Some(7));
        assert_eq!(extract_track_number("0010 Song Title"), Some(10));
    }

    #[test_log::test]
    fn test_extract_track_number_all_zeros_returns_none() {
        // All zeros should return None since skip_while removes them all
        assert_eq!(extract_track_number("000 Song Title"), None);
    }

    #[test_log::test]
    fn test_extract_track_number_without_number_prefix() {
        assert_eq!(extract_track_number("Song Title"), None);
        assert_eq!(extract_track_number("No Number Here"), None);
    }

    #[test_log::test]
    fn test_extract_track_number_with_only_number() {
        assert_eq!(extract_track_number("42"), Some(42));
        assert_eq!(extract_track_number("7"), Some(7));
    }

    #[test_log::test]
    fn test_extract_track_name_skips_leading_numbers_and_separators() {
        assert_eq!(
            extract_track_name("01 Song Title"),
            Some("Song Title".to_string())
        );
        assert_eq!(
            extract_track_name("01-Song Title"),
            Some("Song Title".to_string())
        );
        assert_eq!(
            extract_track_name("01_Song Title"),
            Some("Song Title".to_string())
        );
    }

    #[test_log::test]
    fn test_extract_track_name_with_multiple_separators() {
        assert_eq!(
            extract_track_name("01 - Song Title"),
            Some("Song Title".to_string())
        );
        assert_eq!(
            extract_track_name("01__Song Title"),
            Some("Song Title".to_string())
        );
    }

    #[test_log::test]
    fn test_extract_track_name_converts_underscores_to_spaces() {
        assert_eq!(
            extract_track_name("01_Song_Title"),
            Some("Song Title".to_string())
        );
        assert_eq!(
            extract_track_name("Song_With_Underscores"),
            Some("Song With Underscores".to_string())
        );
    }

    #[test_log::test]
    fn test_extract_track_name_without_number_prefix() {
        assert_eq!(
            extract_track_name("Song Title"),
            Some("Song Title".to_string())
        );
        assert_eq!(
            extract_track_name("Another Song"),
            Some("Another Song".to_string())
        );
    }

    #[test_log::test]
    fn test_extract_track_name_empty_after_skipping() {
        // If only numbers and separators, should return None
        assert_eq!(extract_track_name("01 "), None);
        assert_eq!(extract_track_name("123-_-"), None);
    }

    #[test_log::test]
    fn test_extract_track_name_number_only() {
        // Pure numbers should return None
        assert_eq!(extract_track_name("12345"), None);
    }

    #[test_log::test]
    fn test_music_file_pattern_matches_valid_extensions() {
        assert!(MUSIC_FILE_PATTERN.is_match("song.flac"));
        assert!(MUSIC_FILE_PATTERN.is_match("track.m4a"));
        assert!(MUSIC_FILE_PATTERN.is_match("music.mp3"));
        assert!(MUSIC_FILE_PATTERN.is_match("audio.opus"));
    }

    #[test_log::test]
    fn test_music_file_pattern_rejects_invalid_extensions() {
        assert!(!MUSIC_FILE_PATTERN.is_match("image.jpg"));
        assert!(!MUSIC_FILE_PATTERN.is_match("document.pdf"));
        assert!(!MUSIC_FILE_PATTERN.is_match("video.mp4"));
        assert!(!MUSIC_FILE_PATTERN.is_match("cover.png"));
    }

    #[test_log::test]
    fn test_music_file_pattern_requires_filename_before_extension() {
        // Pattern is ".+\." so requires at least one char before extension
        assert!(!MUSIC_FILE_PATTERN.is_match(".flac"));
        assert!(MUSIC_FILE_PATTERN.is_match("a.flac"));
    }

    #[test_log::test]
    fn test_multi_artist_pattern_matches_comma_between_non_whitespace() {
        // Pattern is "\S,\S" - non-whitespace, comma, non-whitespace
        assert!(MULTI_ARTIST_PATTERN.is_match("Artist1,Artist2"));
        assert!(MULTI_ARTIST_PATTERN.is_match("a,b"));
    }

    #[test_log::test]
    fn test_multi_artist_pattern_no_match_with_spaces_around_comma() {
        // Pattern doesn't match when there are spaces around the comma
        assert!(!MULTI_ARTIST_PATTERN.is_match("Artist1, Artist2"));
        assert!(!MULTI_ARTIST_PATTERN.is_match("Artist1 ,Artist2"));
        assert!(!MULTI_ARTIST_PATTERN.is_match("Artist1 , Artist2"));
    }

    #[test_log::test]
    fn test_multi_artist_pattern_truncation_logic() {
        // Test the actual truncation logic used in scan_track
        // Pattern "\S,\S" matches char + comma + char, so for "Artist1,Artist2"
        // it matches "1,A" starting at position 6 (the '1' char).
        // The slice [..=comma.start()] gives up to and including position 6, which is "Artist1"
        let test_cases = [
            // (input, expected output after truncation)
            ("Artist1,Artist2", "Artist1"),
            ("John Doe,Jane Doe", "John Doe"),
            ("Band1,Band2,Band3", "Band1"),
            // No match cases - should return unchanged
            ("Single Artist", "Single Artist"),
            ("Artist1, Artist2", "Artist1, Artist2"), // Space after comma
            ("Artist1 & Artist2", "Artist1 & Artist2"),
        ];

        for (input, expected) in test_cases {
            let result = MULTI_ARTIST_PATTERN.find(input).map_or_else(
                || input.to_string(),
                |comma| input[..=comma.start()].to_string(),
            );
            assert_eq!(result, expected, "Failed for input: {input}");
        }
    }
}
