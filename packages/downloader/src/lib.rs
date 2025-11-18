#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::redundant_pub_crate)]
#![doc = "Music download management system for the `MoosicBox` ecosystem."]
#![doc = ""]
#![doc = "Provides functionality for downloading music tracks, album covers, and artist covers"]
#![doc = "from various music API sources with queue management, progress tracking, and automatic"]
#![doc = "file tagging."]

use std::{
    error::Error,
    num::ParseIntError,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
    time::Duration,
};

use crate::queue::GenericProgressEvent;
use async_recursion::async_recursion;
use async_trait::async_trait;
use atomic_float::AtomicF64;
use db::{
    create_download_task,
    models::{CreateDownloadTask, DownloadItem, DownloadTask},
};
use futures::StreamExt;
use id3::Timestamp;
use moosicbox_audiotags::Tag;
use moosicbox_config::get_config_dir_path;
use moosicbox_files::{
    GetContentLengthError, SaveBytesStreamToFileError,
    files::{
        album::{AlbumCoverError, get_album_cover_bytes},
        artist::{ArtistCoverError, get_artist_cover_bytes},
        track::{GetTrackBytesError, TrackSourceError, get_track_bytes},
    },
    get_content_length, sanitize_filename, save_bytes_stream_to_file_with_speed_listener,
};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::{
    MusicApi, MusicApis, SourceToMusicApi as _,
    models::{ImageCoverSize, TrackSource},
};
use moosicbox_music_models::{
    Album, ApiSource, Artist, AudioFormat, Track, TrackApiSource,
    id::{Id, ParseIdsError},
};
use moosicbox_remote_library::RemoteLibraryMusicApi;
use queue::{DownloadQueue, ProgressListener};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumDiscriminants, EnumIter, EnumString};
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;
use tokio::{select, sync::RwLock};

pub use db::{
    create_download_location, delete_download_location, get_download_location,
    get_download_locations,
};
pub use moosicbox_music_api::models::TrackAudioQuality;

#[cfg(feature = "api")]
pub mod api;

pub(crate) mod db;
pub mod queue;

static DOWNLOAD_QUEUE: LazyLock<Arc<RwLock<DownloadQueue>>> =
    LazyLock::new(|| Arc::new(RwLock::new(DownloadQueue::new())));

/// Download API source for identifying where to download content from.
///
/// Specifies either a `MoosicBox` server or a third-party API source.
#[derive(
    Debug, Serialize, Deserialize, EnumString, EnumDiscriminants, AsRefStr, PartialEq, Eq, Clone,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[strum_discriminants(derive(EnumIter, Serialize, Deserialize))]
#[strum_discriminants(serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[cfg_attr(feature = "openapi", strum_discriminants(derive(utoipa::ToSchema)))]
#[strum_discriminants(name(DownloadApiSourceDiscriminants))]
#[strum_discriminants(vis(pub))]
#[strum_discriminants(doc = "Discriminant variants for `DownloadApiSource`.")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "source", content = "url")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum DownloadApiSource {
    /// `MoosicBox` server at the specified URL
    MoosicBox(String),
    /// Third-party API source
    Api(ApiSource),
}

impl From<ApiSource> for DownloadApiSource {
    fn from(value: ApiSource) -> Self {
        Self::Api(value)
    }
}

impl From<&DownloadApiSource> for ApiSource {
    fn from(value: &DownloadApiSource) -> Self {
        value.clone().into()
    }
}

impl From<DownloadApiSource> for ApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::MoosicBox(..) => Self::library(),
            DownloadApiSource::Api(source) => source,
        }
    }
}

/// Error converting from `TrackApiSource` to `DownloadApiSource`.
#[derive(Debug, Error)]
pub enum TryFromTrackApiSourceError {
    /// Source is not valid for downloads
    #[error("Invalid source")]
    InvalidSource,
}

impl TryFrom<TrackApiSource> for DownloadApiSource {
    type Error = TryFromTrackApiSourceError;

    fn try_from(value: TrackApiSource) -> Result<Self, Self::Error> {
        #[allow(unreachable_code)]
        Ok(match value {
            TrackApiSource::Api(source) => Self::Api(source),
            TrackApiSource::Local => return Err(Self::Error::InvalidSource),
        })
    }
}

/// Error that can occur during the download process.
#[derive(Debug, Error)]
pub enum DownloadError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to get config directory
    #[error("Failed to get config directory")]
    FailedToGetConfigDirectory,
    /// Download item not found
    #[error("Not found")]
    NotFound,
    /// Failed to get download path
    #[error(transparent)]
    GetDownloadPath(#[from] GetDownloadPathError),
    /// Failed to get create download tasks
    #[error(transparent)]
    GetCreateDownloadTasks(#[from] GetCreateDownloadTasksError),
    /// Failed to create download tasks
    #[error(transparent)]
    CreateDownloadTasks(#[from] CreateDownloadTasksError),
    /// Music API operation failed
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// Request parameters for downloading music content.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// Target directory for downloaded files
    pub directory: PathBuf,
    /// Single track ID to download
    pub track_id: Option<Id>,
    /// Multiple track IDs to download
    pub track_ids: Option<Vec<Id>>,
    /// Single album ID to download
    pub album_id: Option<Id>,
    /// Multiple album IDs to download
    pub album_ids: Option<Vec<Id>>,
    /// Whether to download album cover art
    pub download_album_cover: Option<bool>,
    /// Whether to download artist cover art
    pub download_artist_cover: Option<bool>,
    /// Audio quality for downloaded tracks
    pub quality: Option<TrackAudioQuality>,
    /// API source to download from
    pub source: DownloadApiSource,
}

#[allow(clippy::unnecessary_wraps)]
fn music_api_from_source(
    #[allow(unused)] music_apis: &MusicApis,
    source: DownloadApiSource,
) -> Result<Arc<Box<dyn MusicApi>>, moosicbox_music_api::Error> {
    const PROFILE: &str = "master";

    Ok(match source {
        DownloadApiSource::MoosicBox(host) => Arc::new(Box::new(RemoteLibraryMusicApi::new(
            host,
            ApiSource::library(),
            PROFILE.to_string(),
        ))),
        DownloadApiSource::Api(source) => music_apis
            .get(&source)
            .ok_or_else(|| moosicbox_music_api::Error::MusicApiNotFound(source.clone()))?,
    })
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
pub async fn download(
    request: DownloadRequest,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<(), DownloadError> {
    let tasks = get_create_download_tasks(
        &**music_api_from_source(&music_apis, request.source.clone())?,
        &request.directory,
        request.track_id,
        request.track_ids,
        request.album_id,
        request.album_ids,
        request.download_album_cover.unwrap_or(true),
        request.download_artist_cover.unwrap_or(true),
        request.quality,
        Some(request.source),
    )
    .await?;

    let tasks = create_download_tasks(&db, tasks).await?;

    let queue = get_default_download_queue(db.clone(), music_apis).await;
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    download_queue.process();

    drop(download_queue);

    Ok(())
}

async fn get_default_download_queue(
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Arc<RwLock<DownloadQueue>> {
    let queue = { DOWNLOAD_QUEUE.read().await.clone() };

    if !queue.has_database() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        *queue = queue.clone().with_database(db.clone());
    }
    if !queue.has_downloader() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        *queue = queue
            .clone()
            .with_downloader(Box::new(MoosicboxDownloader::new(db, music_apis)));
    }

    DOWNLOAD_QUEUE.clone()
}

/// Error getting the download path.
#[derive(Debug, Error)]
pub enum GetDownloadPathError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to get config directory
    #[error("Failed to get config directory")]
    FailedToGetConfigDirectory,
    /// Download location not found
    #[error("Not found")]
    NotFound,
}

/// # Panics
///
/// * If the path cannot be created from the download location string
///
/// # Errors
///
/// * If there is a database error
/// * If there is no config dir set
/// * If the download path is not found
pub async fn get_download_path(
    db: &LibraryDatabase,
    location_id: Option<u64>,
) -> Result<PathBuf, GetDownloadPathError> {
    Ok(if let Some(location_id) = location_id {
        PathBuf::from_str(
            &get_download_location(db, location_id)
                .await?
                .ok_or(GetDownloadPathError::NotFound)?
                .path,
        )
        .unwrap()
    } else {
        get_default_download_path()?
    })
}

/// # Errors
///
/// * If the config directory path fails to be retrieved
pub fn get_default_download_path() -> Result<PathBuf, GetDownloadPathError> {
    Ok(get_config_dir_path()
        .ok_or(GetDownloadPathError::FailedToGetConfigDirectory)?
        .join("downloads"))
}

/// Error creating download tasks from track or album IDs.
#[derive(Debug, Error)]
pub enum GetCreateDownloadTasksError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API operation failed
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Failed to parse integer value
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    /// Failed to parse IDs
    #[error(transparent)]
    ParseIds(#[from] ParseIdsError),
    /// Invalid API source
    #[error("Invalid source")]
    InvalidSource,
    /// Track or album not found
    #[error("Not found")]
    NotFound,
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
#[allow(clippy::too_many_arguments)]
pub async fn get_create_download_tasks(
    api: &dyn MusicApi,
    download_path: &Path,
    track_id: Option<Id>,
    track_ids: Option<Vec<Id>>,
    album_id: Option<Id>,
    album_ids: Option<Vec<Id>>,
    download_album_cover: bool,
    download_artist_cover: bool,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    if let Some(album_id) = album_id {
        tasks.extend(
            #[allow(unreachable_code)]
            get_create_download_tasks_for_album_ids(
                api,
                &[Id::try_from_str(&album_id.to_string(), api.source())?],
                download_path,
                source.clone(),
                quality,
                download_album_cover,
                download_artist_cover,
            )
            .await?,
        );
    }

    if let Some(album_ids) = &album_ids {
        #[allow(unreachable_code)]
        tasks.extend(
            get_create_download_tasks_for_album_ids(
                api,
                album_ids,
                download_path,
                source.clone(),
                quality,
                download_album_cover,
                download_artist_cover,
            )
            .await?,
        );
    }

    if let Some(track_id) = track_id {
        tasks.extend(
            #[allow(unreachable_code)]
            get_create_download_tasks_for_track_ids(
                api,
                &[Id::try_from_str(&track_id.to_string(), api.source())?],
                download_path,
                source.clone(),
                quality,
            )
            .await?,
        );
    }

    if let Some(track_ids) = &track_ids {
        tasks.extend(
            get_create_download_tasks_for_track_ids(api, track_ids, download_path, source, quality)
                .await?,
        );
    }

    Ok(tasks)
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
pub async fn get_create_download_tasks_for_track_ids(
    api: &dyn MusicApi,
    track_ids: &[Id],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let tracks = api.tracks(Some(track_ids), None, None, None, None).await?;
    log::debug!(
        "get_create_download_tasks_for_track_ids: track_ids={track_ids:?} tracks={:?}",
        &tracks.items()
    );

    get_create_download_tasks_for_tracks(api, &tracks, download_path, source, quality).await
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
pub async fn get_create_download_tasks_for_tracks(
    api: &dyn MusicApi,
    tracks: &[Track],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    for track in tracks {
        static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

        let source = source
            .clone()
            .unwrap_or_else(|| track.track_source.clone().try_into().unwrap());

        let quality = quality.unwrap_or(TrackAudioQuality::FlacHighestRes);

        let album = api.album(&track.album_id).await?;

        let path = download_path
            .join(sanitize_filename(&track.artist))
            .join(sanitize_filename(&track.album))
            .join(get_filename_for_track(track))
            .to_str()
            .unwrap()
            .to_string();

        let path = if std::env::consts::OS == "windows" {
            REGEX
                .replace(&path, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\")
        } else {
            path
        };

        tasks.push(CreateDownloadTask {
            file_path: path,
            item: DownloadItem::Track {
                track_id: track.id.clone(),
                source,
                quality,
                artist_id: track.artist_id.clone(),
                artist: track.artist.clone(),
                album_id: track.album_id.clone(),
                album: track.album.clone(),
                title: track.title.clone(),
                contains_cover: album.is_some_and(|x| x.artwork.is_some()),
            },
        });
    }

    Ok(tasks)
}

/// # Panics
///
/// * If the album `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn get_create_download_tasks_for_album_ids(
    api: &dyn MusicApi,
    album_ids: &[Id],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
    download_album_cover: bool,
    download_artist_cover: bool,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];
    let source = source
        .clone()
        .unwrap_or_else(|| api.source().clone().into());

    for album_id in album_ids {
        let album = api
            .album(album_id)
            .await?
            .ok_or(GetCreateDownloadTasksError::NotFound)?;

        if download_album_cover || download_artist_cover {
            static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

            let album_path = download_path
                .join(sanitize_filename(&album.artist))
                .join(sanitize_filename(&album.title));

            let path = album_path.join("cover.jpg").to_str().unwrap().to_string();

            let path = if std::env::consts::OS == "windows" {
                REGEX
                    .replace(&path, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\")
            } else {
                path
            };

            if download_artist_cover {
                static REGEX: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

                let artist = api
                    .artist(&album.artist_id)
                    .await?
                    .ok_or(GetCreateDownloadTasksError::NotFound)?;

                let path = album_path
                    .parent()
                    .unwrap()
                    .join("artist.jpg")
                    .to_str()
                    .unwrap()
                    .to_string();

                let path = if std::env::consts::OS == "windows" {
                    REGEX
                        .replace(&path, |caps: &Captures| {
                            format!("{}:", caps[1].to_uppercase())
                        })
                        .replace('/', "\\")
                } else {
                    path
                };

                if artist.cover.is_some() {
                    #[allow(unreachable_code)]
                    tasks.push(CreateDownloadTask {
                        file_path: path,
                        item: DownloadItem::ArtistCover {
                            album_id: album_id.to_owned(),
                            source: source.clone(),
                            artist_id: artist.id,
                            title: artist.title,
                            contains_cover: artist.cover.is_some(),
                        },
                    });
                }
            }

            if download_album_cover && album.artwork.is_some() {
                #[allow(unreachable_code)]
                tasks.push(CreateDownloadTask {
                    file_path: path,
                    item: DownloadItem::AlbumCover {
                        album_id: album_id.to_owned(),
                        source: source.clone(),
                        artist_id: album.artist_id,
                        artist: album.artist,
                        title: album.title,
                        contains_cover: album.artwork.is_some(),
                    },
                });
            }
        }

        let tracks = api
            .album_tracks(album_id, None, None, None, None)
            .await?
            .with_rest_of_items_in_batches()
            .await?
            .into_iter()
            .filter(|track| match source.clone() {
                DownloadApiSource::MoosicBox(_) => track.track_source == TrackApiSource::Local,
                DownloadApiSource::Api(source) => track.track_source == source.into(),
            })
            .collect::<Vec<_>>();

        if tracks.is_empty() {
            continue;
        }

        tasks.extend(
            get_create_download_tasks_for_tracks(
                api,
                &tracks,
                download_path,
                Some(source.clone()),
                quality,
            )
            .await?,
        );
    }

    Ok(tasks)
}

/// Error creating download tasks in the database.
#[derive(Debug, Error)]
pub enum CreateDownloadTasksError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// # Errors
///
/// * If the download tasks fail to be created in the database
pub async fn create_download_tasks(
    db: &LibraryDatabase,
    tasks: Vec<CreateDownloadTask>,
) -> Result<Vec<DownloadTask>, CreateDownloadTasksError> {
    let mut results = vec![];

    for task in tasks {
        results.push(create_download_task(db, &task).await?);
    }

    Ok(results)
}

fn get_filename_for_track(track: &Track) -> String {
    let extension = "flac";

    format!(
        "{}_{}.{extension}",
        track.number,
        sanitize_filename(&track.title)
    )
}

/// Error downloading a track.
#[derive(Debug, Error)]
pub enum DownloadTrackError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API operation failed
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Failed to get track source
    #[error(transparent)]
    TrackSource(#[from] TrackSourceError),
    /// Failed to get track bytes
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    /// I/O operation failed
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    /// Failed to get content length
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    /// Failed to save bytes stream to file
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    /// Failed to tag track file
    #[error(transparent)]
    TagTrackFile(#[from] TagTrackFileError),
    /// Invalid track source
    #[error("Invalid source")]
    InvalidSource,
    /// Track not found
    #[error("Not found")]
    NotFound,
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
#[allow(clippy::too_many_arguments)]
pub async fn download_track_id(
    api: &dyn MusicApi,
    path: &str,
    track_id: &Id,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<Track, DownloadTrackError> {
    log::debug!(
        "Starting download for track_id={track_id} quality={quality:?} source={source:?} path={path}"
    );

    let track = api
        .track(track_id)
        .await?
        .ok_or(DownloadTrackError::NotFound)?;

    download_track(
        api,
        path,
        &track,
        quality,
        None,
        on_progress,
        speed,
        timeout_duration,
    )
    .await?;

    Ok(track)
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
async fn download_track(
    api: &dyn MusicApi,
    path: &str,
    track: &Track,
    quality: TrackAudioQuality,
    start: Option<u64>,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    match download_track_inner(
        api,
        path,
        track,
        quality,
        start,
        on_progress.clone(),
        speed.clone(),
        timeout_duration,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) => Err(match e {
            DownloadTrackInnerError::DatabaseFetch(e) => DownloadTrackError::DatabaseFetch(e),
            DownloadTrackInnerError::MusicApi(e) => DownloadTrackError::MusicApi(e),
            DownloadTrackInnerError::TrackSource(e) => DownloadTrackError::TrackSource(e),
            DownloadTrackInnerError::GetTrackBytes(e) => DownloadTrackError::GetTrackBytes(e),
            DownloadTrackInnerError::IO(e) => DownloadTrackError::IO(e),
            DownloadTrackInnerError::GetContentLength(e) => DownloadTrackError::GetContentLength(e),
            DownloadTrackInnerError::SaveBytesStreamToFile(e) => {
                DownloadTrackError::SaveBytesStreamToFile(e)
            }
            DownloadTrackInnerError::TagTrackFile(e) => DownloadTrackError::TagTrackFile(e),
            DownloadTrackInnerError::InvalidSource => DownloadTrackError::InvalidSource,
            DownloadTrackInnerError::NotFound => DownloadTrackError::NotFound,
            DownloadTrackInnerError::Timeout(start) => {
                log::warn!("Track download timed out. Trying again at start {start:?}");
                return download_track(
                    api,
                    path,
                    track,
                    quality,
                    start,
                    on_progress,
                    speed,
                    timeout_duration,
                )
                .await;
            }
        }),
    }
}

/// Internal error type for track download operations.
///
/// Used internally to handle timeout retries. The `Timeout` variant contains
/// the byte offset for resuming downloads.
#[derive(Debug, Error)]
pub enum DownloadTrackInnerError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API operation failed
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Failed to get track source
    #[error(transparent)]
    TrackSource(#[from] TrackSourceError),
    /// Failed to get track bytes
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    /// I/O operation failed
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Failed to get content length
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    /// Failed to save bytes stream to file
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    /// Failed to tag track file
    #[error(transparent)]
    TagTrackFile(#[from] TagTrackFileError),
    /// Invalid track source
    #[error("Invalid source")]
    InvalidSource,
    /// Track not found
    #[error("Not found")]
    NotFound,
    /// Download timed out, contains byte offset for resume
    #[error("Timeout")]
    Timeout(Option<u64>),
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
async fn download_track_inner(
    api: &dyn MusicApi,
    path: &str,
    track: &Track,
    quality: TrackAudioQuality,
    mut start: Option<u64>,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackInnerError> {
    log::debug!(
        "Starting download for track={track:?} quality={quality:?} path={path} start={start:?}"
    );

    let req = api.track_source(track.into(), quality);

    let result = if let Some(timeout_duration) = timeout_duration {
        select! {
            result = req => result,
            () = tokio::time::sleep(timeout_duration) => {
                return Err(DownloadTrackInnerError::Timeout(start));
            }
        }
    } else {
        req.await
    };

    let source = match result {
        Ok(Some(source)) => source,
        Ok(None) => {
            return Err(DownloadTrackInnerError::InvalidSource);
        }
        Err(e) => {
            let is_timeout = e.source().is_some_and(|source| {
                source.downcast_ref::<hyper::Error>().map_or_else(
                    || source.to_string() == "operation timed out",
                    |error| {
                        error.is_timeout()
                            || error.is_closed()
                            || error.is_canceled()
                            || error.is_incomplete_message()
                    },
                )
            });

            if is_timeout {
                return Err(DownloadTrackInnerError::Timeout(start));
            }

            return Err(e.into());
        }
    };

    let size = match &source {
        TrackSource::LocalFilePath { path, .. } => {
            if let Ok(file) = tokio::fs::File::open(path).await {
                (file.metadata().await).map_or(None, |metadata| Some(metadata.len()))
            } else {
                None
            }
        }
        TrackSource::RemoteUrl { url, .. } => get_content_length(url, start, None).await?,
    };

    log::debug!("Got track size: {size:?}");

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: size }).await;

    let mut bytes = get_track_bytes(
        api,
        &track.id,
        source,
        AudioFormat::Source,
        false,
        start,
        None,
    )
    .await?;

    if let Some(size) = size {
        bytes.size.replace(size);
    }

    let track_path = PathBuf::from_str(path).unwrap();

    tokio::fs::create_dir_all(&track_path.parent().unwrap()).await?;

    if Path::is_file(&track_path) {
        log::debug!("Track already downloaded");
        if start.is_none() {
            if let Ok(metadata) = std::fs::File::open(&track_path).and_then(|x| x.metadata()) {
                let len = metadata.len();
                start = Some(len);
                log::debug!("Resuming track download from {len} bytes");
            } else {
                return Ok(());
            }
        }
    }

    log::debug!(
        "Downloading track to track_path={} start={start:?}",
        track_path.display()
    );

    {
        let mut reader = bytes.stream;

        if let Some(timeout_duration) = timeout_duration {
            reader = reader.with_timeout(timeout_duration);
        }

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        let result = save_bytes_stream_to_file_with_speed_listener(
            reader.map(|x| match x {
                Ok(Ok(x)) => Ok(x),
                Ok(Err(e)) | Err(e) => Err(e),
            }),
            &track_path,
            start,
            Box::new({
                let speed = speed.clone();
                let speed_progress = on_progress.clone();
                move |x| {
                    let speed = speed.clone();
                    let speed_progress = speed_progress.clone();
                    Box::pin(async move {
                        (speed_progress.lock().await)(GenericProgressEvent::Speed {
                            bytes_per_second: x,
                        })
                        .await;
                        speed.store(x, std::sync::atomic::Ordering::SeqCst);
                    })
                }
            }),
            Some(Box::new(move |read, total| {
                let on_progress = on_progress.clone();
                Box::pin(async move {
                    (on_progress.lock().await)(GenericProgressEvent::BytesRead { read, total })
                        .await;
                })
            })),
        )
        .await;

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        if let Err(e) = result {
            if let SaveBytesStreamToFileError::Read {
                bytes_read,
                ref source,
            } = e
                && source.kind() == tokio::io::ErrorKind::TimedOut
            {
                return Err(DownloadTrackInnerError::Timeout(Some(bytes_read)));
            }

            return Err(DownloadTrackInnerError::SaveBytesStreamToFile(e));
        }
    }

    log::debug!(
        "Finished downloading track to track_path={}",
        track_path.display()
    );

    tag_track_file(&track_path, track)?;

    log::debug!(
        "Completed track download for track_path={}",
        track_path.display()
    );

    Ok(())
}

/// Error tagging a track file with metadata.
#[derive(Debug, Error)]
pub enum TagTrackFileError {
    /// Failed to tag audio file
    #[error(transparent)]
    Tag(#[from] moosicbox_audiotags::Error),
}

/// # Panics
///
/// * If the track number fails to be converted to a `u16`
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If `moosicbox_audiotags` fails to tag the audio file
pub fn tag_track_file(track_path: &Path, track: &Track) -> Result<(), TagTrackFileError> {
    log::debug!("Adding tags to track_path={}", track_path.display());

    let mut tag = Tag::new().read_from_path(track_path)?;

    tag.set_title(&track.title);
    tag.set_track_number(u16::try_from(track.number).unwrap());
    tag.set_album_title(&track.album);
    tag.set_artist(&track.artist);
    tag.set_album_artist(&track.artist);

    if let Some(date) = &track.date_released
        && let Ok(timestamp) = Timestamp::from_str(date)
    {
        tag.set_date(timestamp);
    }

    tag.write_to_path(track_path.to_str().unwrap())?;

    Ok(())
}

/// Error downloading an album.
#[derive(Debug, Error)]
pub enum DownloadAlbumError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API operation failed
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Failed to download track
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    /// I/O operation failed
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Failed to save bytes stream to file
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    /// Failed to get artist cover
    #[error(transparent)]
    ArtistCover(#[from] ArtistCoverError),
    /// Failed to get album cover
    #[error(transparent)]
    AlbumCover(#[from] AlbumCoverError),
    /// Invalid album source
    #[error("Invalid source")]
    InvalidSource,
    /// Album not found
    #[error("Not found")]
    NotFound,
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
#[allow(clippy::too_many_arguments)]
pub async fn download_album_id(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    try_download_album_cover: bool,
    try_download_artist_cover: bool,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadAlbumError> {
    log::debug!(
        "Starting download for album_id={album_id} quality={quality:?} source={source:?} path={path}"
    );

    let tracks = api
        .album_tracks(album_id, None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?
        .into_iter()
        .filter(|track| match source.clone() {
            DownloadApiSource::MoosicBox(_) => unimplemented!(),
            DownloadApiSource::Api(source) => track.track_source == source.into(),
        })
        .collect::<Vec<_>>();

    for track in &tracks {
        download_track(
            api,
            path,
            track,
            quality,
            None,
            on_progress.clone(),
            speed.clone(),
            timeout_duration,
        )
        .await?;
    }

    log::debug!("Completed album download for {} tracks", tracks.len());

    if try_download_album_cover {
        download_album_cover(api, db, path, album_id, on_progress.clone(), speed.clone()).await?;
    }

    if try_download_artist_cover {
        download_artist_cover(api, db, path, album_id, on_progress, speed).await?;
    }

    Ok(())
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
pub async fn download_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Album, DownloadAlbumError> {
    log::debug!("Downloading album cover path={path}");

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    let cover_path = PathBuf::from_str(path).unwrap();

    let album = api
        .album(album_id)
        .await?
        .ok_or(DownloadAlbumError::NotFound)?;

    if Path::is_file(&cover_path) {
        log::debug!("Album cover already downloaded");
        return Ok(album);
    }

    let bytes = match get_album_cover_bytes(api, db, &album, ImageCoverSize::Max, true).await {
        Ok(bytes) => bytes,
        Err(e) => match e {
            AlbumCoverError::NotFound(_) => {
                log::debug!("No album cover found");
                return Ok(album);
            }
            _ => {
                return Err(DownloadAlbumError::AlbumCover(e));
            }
        },
    };

    log::debug!("Got album cover size: {:?}", bytes.size);

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: bytes.size }).await;

    log::debug!("Saving album cover to {}", cover_path.display());

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) | Err(e) => Err(e),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| {
                let speed = speed.clone();
                Box::pin(async move { speed.store(x, std::sync::atomic::Ordering::SeqCst) })
            }
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed album cover download");

    Ok(album)
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
pub async fn download_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Artist, DownloadAlbumError> {
    log::debug!("Downloading artist cover path={path}");

    let cover_path = PathBuf::from_str(path).unwrap();

    let artist = api
        .album_artist(album_id)
        .await?
        .ok_or(DownloadAlbumError::NotFound)?;

    if Path::is_file(&cover_path) {
        log::debug!("Artist cover already downloaded");
        return Ok(artist);
    }

    let bytes = match get_artist_cover_bytes(api, db, &artist, ImageCoverSize::Max, true).await {
        Ok(bytes) => bytes,
        Err(e) => match e {
            ArtistCoverError::NotFound(..) => {
                log::debug!("No artist cover found");
                return Ok(artist);
            }
            _ => {
                return Err(DownloadAlbumError::ArtistCover(e));
            }
        },
    };

    log::debug!("Got artist cover size: {:?}", bytes.size);

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: bytes.size }).await;

    log::debug!("Saving artist cover to {}", cover_path.display());

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) | Err(e) => Err(e),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| {
                let speed = speed.clone();
                Box::pin(async move { speed.store(x, std::sync::atomic::Ordering::SeqCst) })
            }
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed artist cover download");

    Ok(artist)
}

/// Trait for downloading music content.
///
/// Implementers provide functionality to download tracks, album covers, and artist covers.
#[async_trait]
pub trait Downloader {
    /// Returns the current download speed in bytes per second, if available.
    fn speed(&self) -> Option<f64> {
        None
    }

    /// Downloads a track by ID.
    ///
    /// # Errors
    ///
    /// * If there is a database error
    /// * If there are errors fetching track info
    /// * If failed to fetch the track source
    /// * If failed to add tags to the downloaded audio file
    /// * If an IO error occurs
    /// * If there is an error saving the bytes stream to the file
    /// * If failed to get the content length of the audio data to download
    /// * If given an invalid `ApiSource`
    /// * If the track is not found
    async fn download_track_id(
        &self,
        path: &str,
        track_id: &Id,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<Track, DownloadTrackError>;

    /// Downloads an album cover by album ID.
    ///
    /// # Errors
    ///
    /// * If there is a database error
    /// * If there are errors fetching album info
    /// * If an IO error occurs
    /// * If there is an error saving the bytes stream to the file
    /// * If the album is not found
    async fn download_album_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Album, DownloadAlbumError>;

    /// Downloads an artist cover by album ID.
    ///
    /// # Errors
    ///
    /// * If there is a database error
    /// * If there are errors fetching artist info
    /// * If an IO error occurs
    /// * If there is an error saving the bytes stream to the file
    /// * If the artist is not found
    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Artist, DownloadAlbumError>;
}

/// `MoosicBox` implementation of the `Downloader` trait.
///
/// Provides music downloading functionality using the `MoosicBox` music APIs.
pub struct MoosicboxDownloader {
    speed: Arc<AtomicF64>,
    db: LibraryDatabase,
    music_apis: MusicApis,
}

impl MoosicboxDownloader {
    /// Creates a new `MoosicboxDownloader` instance.
    #[must_use]
    pub fn new(db: LibraryDatabase, music_apis: MusicApis) -> Self {
        Self {
            speed: Arc::new(AtomicF64::new(0.0)),
            db,
            music_apis,
        }
    }
}

#[async_trait]
impl Downloader for MoosicboxDownloader {
    fn speed(&self) -> Option<f64> {
        Some(self.speed.load(std::sync::atomic::Ordering::SeqCst))
    }

    async fn download_track_id(
        &self,
        path: &str,
        track_id: &Id,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<Track, DownloadTrackError> {
        download_track_id(
            &**music_api_from_source(&self.music_apis, source.clone())?,
            path,
            track_id,
            quality,
            source,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
            timeout_duration,
        )
        .await
    }

    async fn download_album_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Album, DownloadAlbumError> {
        download_album_cover(
            &**music_api_from_source(&self.music_apis, source.clone())?,
            &self.db,
            path,
            album_id,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }

    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Artist, DownloadAlbumError> {
        download_artist_cover(
            &**music_api_from_source(&self.music_apis, source.clone())?,
            &self.db,
            path,
            album_id,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static TIDAL_API_SOURCE: LazyLock<ApiSource> =
        LazyLock::new(|| ApiSource::register("Tidal", "Tidal"));

    #[test_log::test]
    fn can_deserialize_and_serialize_moosicbox_download_api_source() {
        let serialized =
            serde_json::to_string(&DownloadApiSource::MoosicBox("test".to_string())).unwrap();
        log::debug!("serialized: {serialized}");
        serde_json::from_str::<DownloadApiSource>(&serialized).unwrap();
    }

    #[test_log::test]
    fn can_deserialize_and_serialize_api_download_api_source() {
        let serialized =
            serde_json::to_string(&DownloadApiSource::Api(TIDAL_API_SOURCE.clone())).unwrap();
        log::debug!("serialized: {serialized}");
        serde_json::from_str::<DownloadApiSource>(&serialized).unwrap();
    }
}
