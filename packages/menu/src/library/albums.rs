use std::sync::{Arc, PoisonError};

use moosicbox_date_utils::chrono;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{
    LibraryAlbumTracksError, LibraryMusicApi,
    db::{delete_track_sizes_by_track_id, delete_tracks},
    models::LibraryAlbum,
};
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

#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    #[error("Poison error")]
    Poison,
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

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

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error(transparent)]
    GetAlbums(#[from] super::GetAlbumsError),
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// # Errors
///
/// * If the `MusicApi` fails to get the albums from the `ApiSource`
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

#[derive(Debug, Error)]
pub enum GetAlbumVersionsError {
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    #[error(transparent)]
    LibraryAlbumTracks(#[from] LibraryAlbumTracksError),
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
}

/// # Errors
///
/// * If the `MusicApi` fails to get the album versions from the `ApiSource`
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

#[derive(Debug, Error)]
pub enum AddAlbumError {
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    #[error(transparent)]
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    UpdateDatabase(#[from] moosicbox_scan::output::UpdateDatabaseError),
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

/// # Errors
///
/// * If the `LibraryMusicApi` fails to add the album to the library
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

#[derive(Debug, Error)]
pub enum RemoveAlbumError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    #[error(transparent)]
    DeleteFromIndex(#[from] DeleteFromIndexError),
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
    #[error(transparent)]
    TryFromId(#[from] TryFromIdError),
}

/// # Errors
///
/// * If the `LibraryMusicApi` fails to remove the album from the library
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

#[derive(Debug, Error)]
pub enum ReFavoriteAlbumError {
    #[error(transparent)]
    AddAlbum(#[from] AddAlbumError),
    #[error(transparent)]
    RemoveAlbum(#[from] RemoveAlbumError),
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
    #[error("Missing album")]
    MissingAlbum,
    #[error("Missing artist")]
    MissingArtist,
    #[error("No artist")]
    NoArtist,
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

/// # Errors
///
/// * If the `LibraryMusicApi` fails to refavorite the album in the library
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
