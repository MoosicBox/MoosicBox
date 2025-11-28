//! Album management operations for the music library.
//!
//! This module provides functionality for working with albums, including fetching
//! albums from various sources, managing album versions, and adding/removing albums
//! from the library. It coordinates between external music APIs and the local database.

use std::sync::{Arc, PoisonError};

use moosicbox_date_utils::chrono;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{
    LibraryAlbumTracksError,
    db::{delete_track_sizes_by_track_id, delete_tracks},
    models::LibraryAlbum,
};
use moosicbox_library_music_api::LibraryMusicApi;
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{MusicApi, SourceToMusicApi as _, models::AlbumsRequest};
use moosicbox_music_models::{
    Album, ApiSource, Artist, TrackApiSource,
    id::{Id, TryFromIdError},
};
use moosicbox_paging::Page;
use moosicbox_scan::{music_api::ScanError, output::ScanOutput};
use moosicbox_search::{
    DeleteFromIndexError, PopulateIndexError,
    data::{AsDataValues, AsDeleteTerm},
};
use moosicbox_session::delete_session_playlist_tracks_by_track_id;
use switchy_database::{DatabaseError, profiles::LibraryDatabase, query::FilterableQuery};
use thiserror::Error;
use tokio::sync::RwLock;

use super::{GetAlbumError, get_albums};

/// Error types that can occur when retrieving album tracks.
#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    /// Lock poisoning error
    #[error("Poison error")]
    Poison,
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

/// Propagates API sources from a library album to an album fetched from an API source.
///
/// This function matches the given album against library albums and copies the
/// `album_sources` and `artist_sources` fields if a match is found. This ensures
/// that albums fetched from external APIs retain their connections to the local library.
pub fn propagate_api_sources_from_library_album<'a>(
    source: &ApiSource,
    album: &'a mut Album,
    library_albums: &[LibraryAlbum],
) -> &'a mut Album {
    let library_album = library_albums.iter().find(|y| {
        y.album_sources
            .iter()
            .any(|z| &z.source == source && z.id == album.id)
    });

    if let Some(library_album) = library_album {
        album.album_sources = library_album.album_sources.clone();
        album.artist_sources = library_album.artist_sources.clone();
    }

    album
}

/// Error types that can occur when retrieving albums from a source.
#[derive(Debug, Error)]
pub enum GetAlbumsError {
    /// Error retrieving albums
    #[error(transparent)]
    GetAlbums(#[from] super::GetAlbumsError),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// Retrieves a paginated list of albums from a music API source.
///
/// Fetches albums from the specified API, applying filters and pagination from the
/// request. For external sources, propagates any existing library album associations
/// to maintain cross-reference information between the library and external APIs.
///
/// # Errors
///
/// * `GetAlbumsError::GetAlbums` if fetching the album list from the database fails
/// * `GetAlbumsError::MusicApi` if the music API fails to retrieve albums
pub async fn get_albums_from_source(
    db: &LibraryDatabase,
    api: &dyn MusicApi,
    request: AlbumsRequest,
) -> Result<Page<Album>, GetAlbumsError> {
    let mut albums =
        if let Some(artist_id) = request.filters.as_ref().and_then(|x| x.artist_id.as_ref()) {
            let album_type = request.filters.as_ref().and_then(|x| x.album_type);
            let offset = request.page.as_ref().map(|x| x.offset);
            let limit = request.page.as_ref().map(|x| x.limit);
            api.artist_albums(artist_id, album_type, offset, limit, None, None)
                .await?
                .page
        } else {
            api.albums(&request).await?.page
        };

    let source = api.source().clone();

    if !source.is_library() {
        let library_albums = get_albums(db).await?;

        albums = albums.map(move |mut album| {
            propagate_api_sources_from_library_album(&source, &mut album, &library_albums);

            album
        });
    }

    Ok(albums)
}

/// Error types that can occur when retrieving album versions.
#[derive(Debug, Error)]
pub enum GetAlbumVersionsError {
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Error retrieving library album tracks
    #[error(transparent)]
    LibraryAlbumTracks(#[from] LibraryAlbumTracksError),
    /// Unknown API source
    #[error("Unknown source: {album_source:?}")]
    UnknownSource {
        /// The unknown source name
        album_source: String,
    },
}

/// Retrieves all available versions of an album from a source.
///
/// Fetches different versions of an album (e.g., different quality formats, bit depths)
/// from the specified source. For library albums, returns all locally available versions.
/// For external API sources, constructs a version list from the album's available tracks.
///
/// # Errors
///
/// * `GetAlbumVersionsError::MusicApi` if the music API fails to retrieve album versions
/// * `GetAlbumVersionsError::LibraryAlbumTracks` if fetching library album tracks fails
/// * `GetAlbumVersionsError::UnknownSource` if the specified API source is not recognized
pub async fn get_album_versions_from_source(
    #[allow(unused)] db: &LibraryDatabase,
    library_api: &LibraryMusicApi,
    profile: &str,
    album_id: &Id,
    source: ApiSource,
) -> Result<Vec<AlbumVersion>, GetAlbumVersionsError> {
    log::trace!("get_album_versions_from_source: album_id={album_id} source={source}");

    #[allow(unreachable_code)]
    Ok(if source.is_library() {
        library_api.library_album_versions(album_id).await?
    } else {
        let music_api = moosicbox_music_api::profiles::PROFILES
            .get(profile)
            .ok_or_else(|| GetAlbumVersionsError::UnknownSource {
                album_source: source.to_string(),
            })?
            .get(&source)
            .ok_or_else(|| GetAlbumVersionsError::UnknownSource {
                album_source: source.to_string(),
            })?;

        let tracks = music_api
            .album_tracks(album_id, None, None, None, None)
            .await?
            .into_items();

        vec![AlbumVersion {
            tracks,
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: source.into(),
        }]
    })
}

/// Error types that can occur when adding an album to the library.
#[derive(Debug, Error)]
pub enum AddAlbumError {
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Error retrieving album
    #[error(transparent)]
    GetAlbum(#[from] GetAlbumError),
    /// Error updating database
    #[error(transparent)]
    UpdateDatabase(#[from] moosicbox_scan::output::UpdateDatabaseError),
    /// Scan error
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// Error populating search index
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    /// Date/time parsing error
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    /// Album not found
    #[error("No album")]
    NoAlbum,
    /// Invalid album ID type
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

/// Adds an album from an external source to the local library.
///
/// Fetches an album and all its tracks from the specified external music API and
/// imports them into the local library database. This includes scanning the album
/// metadata, updating the database, clearing caches, and populating the search index
/// with the new artists, albums, and tracks.
///
/// # Errors
///
/// * `AddAlbumError::MusicApi` if the music API fails to retrieve the album or tracks
/// * `AddAlbumError::NoAlbum` if the album is not found at the source
/// * `AddAlbumError::Scan` if scanning the album metadata fails
/// * `AddAlbumError::UpdateDatabase` if updating the database with the album fails
/// * `AddAlbumError::PopulateIndex` if populating the search index fails
/// * `AddAlbumError::GetAlbum` if retrieving the added album from the library fails
pub async fn add_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<LibraryAlbum, AddAlbumError> {
    log::debug!(
        "add_album: Adding album to library album_id={album_id:?} source={}",
        api.source()
    );

    if let Some(album) = library_api
        .library_album_from_source(album_id, api.source())
        .await?
    {
        log::debug!("Album album_id={album_id:?} already added: album={album:?}");
        return Ok(album);
    }

    let output = Arc::new(RwLock::new(ScanOutput::new()));

    log::debug!(
        "add_album: Fetching album_id={album_id} from {} api",
        api.source()
    );
    let album = api.album(album_id).await?.ok_or(AddAlbumError::NoAlbum)?;
    log::debug!("add_album: Got album={album:?}");

    api.add_album(album_id).await?;

    moosicbox_scan::music_api::scan_albums(api, &[album], 1, output.clone(), None, None).await?;

    let output = output.read().await;
    let results = output.update_database(db).await?;

    moosicbox_library::cache::clear_cache();

    moosicbox_search::populate_global_search_index(
        &results
            .artists
            .clone()
            .into_iter()
            .map(Into::into)
            .map(|artist: Artist| artist.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )
    .await?;

    let mut albums = vec![];

    for album in &results.albums {
        if let Some(album) =
            crate::library::get_library_album(db, &album.id.into(), &ApiSource::library()).await?
        {
            albums.push(album);
        }
    }

    moosicbox_search::populate_global_search_index(
        &albums
            .clone()
            .into_iter()
            .map(TryInto::try_into)
            .map(|album: Result<Album, _>| album.map(|x| x.as_data_values()))
            .collect::<Result<Vec<_>, _>>()?,
        false,
    )
    .await?;

    let tracks = library_api
        .tracks(
            Some(
                &results
                    .tracks
                    .iter()
                    .map(|t| t.id.into())
                    .collect::<Vec<_>>(),
            ),
            None,
            None,
            None,
            None,
        )
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    drop(output);

    moosicbox_search::populate_global_search_index(
        &tracks
            .iter()
            .map(AsDataValues::as_data_values)
            .collect::<Vec<_>>(),
        false,
    )
    .await?;

    if let Some(album) = albums.into_iter().next() {
        return Ok(album);
    }

    library_api
        .library_album_from_source(album_id, api.source())
        .await?
        .ok_or(AddAlbumError::NoAlbum)
}

/// Error types that can occur when removing an album from the library.
#[derive(Debug, Error)]
pub enum RemoveAlbumError {
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Database fetch error
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Error deleting from search index
    #[error(transparent)]
    DeleteFromIndex(#[from] DeleteFromIndexError),
    /// Date/time parsing error
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    /// Album not found
    #[error("No album")]
    NoAlbum,
    /// Invalid album ID type
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
    /// Error converting ID types
    #[error(transparent)]
    TryFromId(#[from] TryFromIdError),
}

/// Removes an album from the local library.
///
/// Deletes an album and its associated tracks from the library database, including
/// removing tracks from session playlists, track size records, API source mappings,
/// and search index entries. If the album has local tracks or other API source
/// associations, the album record is retained but the specified source is removed.
///
/// # Errors
///
/// * `RemoveAlbumError::MusicApi` if the music API fails during album removal
/// * `RemoveAlbumError::NoAlbum` if the album is not found in the library
/// * `RemoveAlbumError::Database` if database operations fail
/// * `RemoveAlbumError::DeleteFromIndex` if removing from the search index fails
/// * `RemoveAlbumError::TryFromId` if ID type conversion fails
#[allow(clippy::too_many_lines)]
pub async fn remove_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<LibraryAlbum, RemoveAlbumError> {
    log::debug!(
        "Removing album from library album_id={album_id:?} source={}",
        api.source()
    );

    #[allow(unused_mut)]
    let mut album = library_api
        .library_album_from_source(album_id, api.source())
        .await?
        .ok_or(RemoveAlbumError::NoAlbum)?;

    log::debug!("Removing album from library album={album:?}");

    if let Err(err) = api.remove_album(album_id).await {
        log::error!("Failed to remove album from MusicApi: {err:?}");
    }

    let tracks = library_api
        .album_tracks(&album.id.into(), None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    let has_local_tracks = tracks
        .iter()
        .any(|track| track.track_source == TrackApiSource::Local);

    let target_tracks = tracks
        .into_iter()
        .filter(|track| match &track.track_source {
            TrackApiSource::Local => false,
            TrackApiSource::Api(source) => album.album_sources.iter().any(|x| &x.source == source),
        })
        .collect::<Vec<_>>();

    let track_ids = target_tracks
        .iter()
        .map(|t| t.id.clone())
        .collect::<Vec<_>>();

    log::debug!("Deleting track db items: {track_ids:?}");
    delete_session_playlist_tracks_by_track_id(
        db,
        Some(
            &track_ids
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    )
    .await?;
    delete_track_sizes_by_track_id(
        db,
        Some(
            &track_ids
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    )
    .await?;
    delete_tracks(
        db,
        Some(
            &track_ids
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    )
    .await?;

    db.delete("api_sources")
        .where_eq("entity_type", "albums")
        .where_eq("entity_id", album.id)
        .execute(&**db)
        .await?;

    moosicbox_library::cache::clear_cache();

    moosicbox_search::delete_from_global_search_index(
        &target_tracks
            .iter()
            .map(AsDeleteTerm::as_delete_term)
            .collect::<Vec<_>>(),
    )?;

    if has_local_tracks
        || album
            .album_sources
            .iter()
            .any(|x| x.source != ApiSource::library())
    {
        log::debug!("Album has other sources, keeping LibraryAlbum");
        return Ok(album);
    }

    log::debug!("Deleting album db item: {}", album.id);
    db.delete("albums")
        .where_eq("id", album.id)
        .execute(&**db)
        .await?;

    {
        let album: Album = album.clone().try_into()?;

        moosicbox_search::delete_from_global_search_index(&[album.as_delete_term()])?;
    }

    Ok(album)
}

/// Error types that can occur when re-favoriting an album.
#[derive(Debug, Error)]
pub enum ReFavoriteAlbumError {
    /// Error adding album
    #[error(transparent)]
    AddAlbum(#[from] AddAlbumError),
    /// Error removing album
    #[error(transparent)]
    RemoveAlbum(#[from] RemoveAlbumError),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Date/time parsing error
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    /// Album not found in current favorites
    #[error("Missing album")]
    MissingAlbum,
    /// Artist not found for album
    #[error("Missing artist")]
    MissingArtist,
    /// Artist not found
    #[error("No artist")]
    NoArtist,
    /// Album not found
    #[error("No album")]
    NoAlbum,
    /// Invalid album ID type
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

/// Re-favorites an album by removing and re-adding it with updated metadata.
///
/// Removes the current version of an album from the library and adds the latest
/// version from the external source. This is useful when an album has been updated
/// at the source (e.g., remastered version, changed track list) and needs to be
/// refreshed in the library. The function matches albums by artist and title to
/// find the updated version.
///
/// # Errors
///
/// * `ReFavoriteAlbumError::MusicApi` if the music API fails to retrieve album information
/// * `ReFavoriteAlbumError::MissingAlbum` if the album is not found in the source's favorites
/// * `ReFavoriteAlbumError::MissingArtist` if the artist information is not available
/// * `ReFavoriteAlbumError::NoAlbum` if the updated album cannot be found at the source
/// * `ReFavoriteAlbumError::RemoveAlbum` if removing the old album fails
/// * `ReFavoriteAlbumError::AddAlbum` if adding the new album fails
pub async fn refavorite_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<LibraryAlbum, ReFavoriteAlbumError> {
    log::debug!(
        "Re-favoriting album from library album_id={album_id:?} source={}",
        api.source()
    );

    let existing: Option<Album> = library_api
        .library_album_from_source(album_id, api.source())
        .await?
        .map(TryInto::try_into)
        .transpose()?;

    let (artist, album) = if let Some(album) = existing {
        if let Some(artist_id) = album.artist_sources.get(api.source()) {
            (
                api.artist(artist_id)
                    .await?
                    .ok_or(ReFavoriteAlbumError::MissingArtist)?,
                album,
            )
        } else {
            return Err(ReFavoriteAlbumError::MissingArtist);
        }
    } else {
        let favorite_albums = api
            .albums(&AlbumsRequest::default())
            .await?
            .with_rest_of_items_in_batches()
            .await?;

        let album = favorite_albums
            .into_iter()
            .find(|album| &album.id == album_id)
            .ok_or(ReFavoriteAlbumError::MissingAlbum)?;

        (
            api.artist(&album.artist_id)
                .await?
                .ok_or(ReFavoriteAlbumError::NoArtist)?,
            album,
        )
    };

    #[allow(clippy::suspicious_operation_groupings)]
    let new_album_id = api
        .artist_albums(&artist.id, None, None, None, None, None)
        .await?
        .with_rest_of_items()
        .await?
        .iter()
        .find(|x| {
            x.artist_id == artist.id
                && x.title.to_lowercase().trim() == album.title.to_lowercase().trim()
        })
        .map(|x| x.id.clone());

    let Some(new_album_id) = new_album_id else {
        log::debug!("No corresponding album to re-favorite album_id={album_id}");
        return Err(ReFavoriteAlbumError::NoAlbum);
    };

    log::debug!("Re-favoriting with ids album_id={album_id} new_album_id={new_album_id:?}");

    remove_album(api, library_api, db, album_id).await?;
    let album = add_album(api, library_api, db, &new_album_id).await?;

    Ok(album)
}

#[cfg(test)]
mod tests {
    use moosicbox_library::models::LibraryAlbumType;
    use moosicbox_music_models::{AlbumSource, AlbumType, ApiSource, ApiSources, id::Id};

    use super::*;

    fn create_test_album(id: Id) -> Album {
        Album {
            id,
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(1),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: None,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        }
    }

    fn create_test_library_album(id: u64, album_sources: ApiSources) -> LibraryAlbum {
        LibraryAlbum {
            id,
            title: "Library Album".to_string(),
            artist: "Library Artist".to_string(),
            artist_id: 1,
            album_type: LibraryAlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: None,
            directory: None,
            source: AlbumSource::Local,
            blur: false,
            versions: vec![],
            album_sources,
            artist_sources: ApiSources::default()
                .with_source(ApiSource::library(), Id::Number(1))
                .with_source(
                    ApiSource::register("Tidal", "Tidal"),
                    Id::String("artist123".to_string()),
                ),
        }
    }

    #[test_log::test]
    fn test_propagate_api_sources_when_library_album_matches() {
        let tidal_source = ApiSource::register("Tidal", "Tidal");
        let album_id = Id::String("tidal_album_123".to_string());

        let mut album = create_test_album(album_id.clone());

        // Library album has album_sources that include the tidal source + matching ID
        let library_album_sources = ApiSources::default()
            .with_source(ApiSource::library(), Id::Number(100))
            .with_source(tidal_source.clone(), album_id);
        let library_album = create_test_library_album(100, library_album_sources.clone());

        let library_albums = vec![library_album];

        // Before propagation, album has empty sources
        assert!(album.album_sources.iter().count() == 0);
        assert!(album.artist_sources.iter().count() == 0);

        propagate_api_sources_from_library_album(&tidal_source, &mut album, &library_albums);

        // After propagation, album_sources and artist_sources should be copied from library album
        assert_eq!(album.album_sources, library_album_sources);
        assert!(album.artist_sources.get(&ApiSource::library()).is_some());
        assert!(album.artist_sources.get(&tidal_source).is_some());
    }

    #[test_log::test]
    fn test_propagate_api_sources_no_match_when_different_source() {
        let tidal_source = ApiSource::register("Tidal", "Tidal");
        let qobuz_source = ApiSource::register("Qobuz", "Qobuz");
        let album_id = Id::String("album_123".to_string());

        let mut album = create_test_album(album_id.clone());

        // Library album has album_sources for Qobuz, not Tidal
        let library_album_sources = ApiSources::default()
            .with_source(ApiSource::library(), Id::Number(100))
            .with_source(qobuz_source, album_id);
        let library_album = create_test_library_album(100, library_album_sources);

        let library_albums = vec![library_album];

        propagate_api_sources_from_library_album(&tidal_source, &mut album, &library_albums);

        // Sources should remain empty since there's no match for Tidal source
        assert!(album.album_sources.iter().count() == 0);
        assert!(album.artist_sources.iter().count() == 0);
    }

    #[test_log::test]
    fn test_propagate_api_sources_no_match_when_different_id() {
        let tidal_source = ApiSource::register("Tidal", "Tidal");
        let album_id = Id::String("album_123".to_string());
        let different_id = Id::String("album_456".to_string());

        let mut album = create_test_album(album_id);

        // Library album has album_sources with same source but different ID
        let library_album_sources = ApiSources::default()
            .with_source(ApiSource::library(), Id::Number(100))
            .with_source(tidal_source.clone(), different_id);
        let library_album = create_test_library_album(100, library_album_sources);

        let library_albums = vec![library_album];

        propagate_api_sources_from_library_album(&tidal_source, &mut album, &library_albums);

        // Sources should remain empty since IDs don't match
        assert!(album.album_sources.iter().count() == 0);
        assert!(album.artist_sources.iter().count() == 0);
    }

    #[test_log::test]
    fn test_propagate_api_sources_empty_library_albums() {
        let tidal_source = ApiSource::register("Tidal", "Tidal");
        let album_id = Id::String("tidal_album_123".to_string());

        let mut album = create_test_album(album_id);
        let library_albums: Vec<LibraryAlbum> = vec![];

        propagate_api_sources_from_library_album(&tidal_source, &mut album, &library_albums);

        // Sources should remain empty since there are no library albums
        assert!(album.album_sources.iter().count() == 0);
        assert!(album.artist_sources.iter().count() == 0);
    }

    #[test_log::test]
    fn test_propagate_api_sources_finds_first_match_among_multiple_albums() {
        let tidal_source = ApiSource::register("Tidal", "Tidal");
        let album_id = Id::String("tidal_album_123".to_string());

        let mut album = create_test_album(album_id.clone());

        // Create multiple library albums, only one matches
        let non_matching_sources = ApiSources::default()
            .with_source(ApiSource::library(), Id::Number(200))
            .with_source(tidal_source.clone(), Id::String("different_id".to_string()));
        let non_matching_album = create_test_library_album(200, non_matching_sources);

        let matching_sources = ApiSources::default()
            .with_source(ApiSource::library(), Id::Number(100))
            .with_source(tidal_source.clone(), album_id);
        let matching_album = create_test_library_album(100, matching_sources.clone());

        let library_albums = vec![non_matching_album, matching_album];

        propagate_api_sources_from_library_album(&tidal_source, &mut album, &library_albums);

        // Should match the second album and copy its sources
        assert_eq!(album.album_sources, matching_sources);
    }

    #[test_log::test]
    fn test_propagate_api_sources_with_numeric_id() {
        let library_source = ApiSource::library();
        let album_id = Id::Number(42);

        let mut album = create_test_album(album_id.clone());

        let library_album_sources = ApiSources::default()
            .with_source(ApiSource::library(), album_id)
            .with_source(
                ApiSource::register("Tidal", "Tidal"),
                Id::String("tidal_id".to_string()),
            );
        let library_album = create_test_library_album(42, library_album_sources.clone());

        let library_albums = vec![library_album];

        propagate_api_sources_from_library_album(&library_source, &mut album, &library_albums);

        // Should match using numeric ID
        assert_eq!(album.album_sources, library_album_sources);
    }
}
