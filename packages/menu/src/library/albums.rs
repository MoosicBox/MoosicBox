use std::sync::{Arc, PoisonError};

use moosicbox_core::sqlite::{
    db::DbError,
    models::{Album, ApiSource, Artist, Id, Track, TrackApiSource},
};
use moosicbox_database::{
    profiles::LibraryDatabase, query::FilterableQuery, DatabaseError, DatabaseValue,
};
use moosicbox_library::{
    db::{delete_track_sizes_by_track_id, delete_tracks},
    models::LibraryAlbum,
    LibraryAlbumTracksError, LibraryMusicApi,
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    models::AlbumsRequest, AlbumsError, ArtistAlbumsError, LibraryAlbumError, MusicApi, TracksError,
};
use moosicbox_paging::Page;
use moosicbox_scan::{music_api::ScanError, output::ScanOutput};
use moosicbox_search::{
    data::{AsDataValues, AsDeleteTerm},
    DeleteFromIndexError, PopulateIndexError,
};
use moosicbox_session::delete_session_playlist_tracks_by_track_id;
use thiserror::Error;
use tokio::sync::RwLock;

use super::{get_albums, GetAlbumError};

#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Db(#[from] DbError),
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub fn propagate_api_sources_from_library_album<'a>(
    source: ApiSource,
    album: &'a mut Album,
    library_albums: &[LibraryAlbum],
) -> &'a mut Album {
    let library_album = library_albums.iter().find(|y| {
        y.album_sources
            .iter()
            .any(|z| z.source == source && z.id == album.id)
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
    Albums(#[from] AlbumsError),
    #[error(transparent)]
    ArtistAlbums(#[from] ArtistAlbumsError),
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

    let source = api.source();

    if source != ApiSource::Library {
        let library_albums = get_albums(db).await?;

        albums = albums.map(move |mut album| {
            propagate_api_sources_from_library_album(source, &mut album, &library_albums);

            album
        });
    }

    Ok(albums)
}

#[derive(Debug, Error)]
pub enum GetAlbumVersionsError {
    #[error(transparent)]
    Tracks(#[from] TracksError),
    #[error(transparent)]
    LibraryAlbumTracks(#[from] LibraryAlbumTracksError),
    #[cfg(feature = "tidal")]
    #[error(transparent)]
    TidalAlbumTracks(#[from] moosicbox_tidal::TidalAlbumTracksError),
    #[cfg(feature = "qobuz")]
    #[error(transparent)]
    QobuzAlbumTracks(#[from] moosicbox_qobuz::QobuzAlbumTracksError),
    #[cfg(feature = "yt")]
    #[error(transparent)]
    YtAlbumTracks(#[from] moosicbox_yt::YtAlbumTracksError),
}

/// # Errors
///
/// * If the `MusicApi` fails to get the album versions from the `ApiSource`
pub async fn get_album_versions_from_source(
    #[allow(unused)] db: &LibraryDatabase,
    library_api: &LibraryMusicApi,
    album_id: &Id,
    source: ApiSource,
) -> Result<Vec<AlbumVersion>, GetAlbumVersionsError> {
    log::trace!("get_album_versions_from_source: album_id={album_id} source={source}");

    #[allow(unreachable_code)]
    Ok(if source == ApiSource::Library {
        library_api.library_album_versions(album_id).await?
    } else {
        #[allow(unused)]
        let tracks: Vec<Track> = match source {
            ApiSource::Library => unreachable!(),
            #[cfg(feature = "tidal")]
            ApiSource::Tidal => {
                moosicbox_tidal::album_tracks(db, album_id, None, None, None, None, None, None)
                    .await?
                    .into_items()
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
            }

            #[cfg(feature = "qobuz")]
            ApiSource::Qobuz => moosicbox_qobuz::album_tracks(db, album_id, None, None, None, None)
                .await?
                .into_items()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            #[cfg(feature = "yt")]
            ApiSource::Yt => {
                moosicbox_yt::album_tracks(db, album_id, None, None, None, None, None, None)
                    .await?
                    .into_items()
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
            }
        };
        vec![AlbumVersion {
            tracks,
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: match source {
                ApiSource::Library => unreachable!(),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => TrackApiSource::Tidal,
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => TrackApiSource::Qobuz,
                #[cfg(feature = "yt")]
                ApiSource::Yt => TrackApiSource::Yt,
            },
        }]
    })
}

#[derive(Debug, Error)]
pub enum AddAlbumError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Album(#[from] moosicbox_music_api::AlbumError),
    #[error(transparent)]
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    Tracks(#[from] moosicbox_music_api::TracksError),
    #[error(transparent)]
    AddAlbum(#[from] moosicbox_music_api::AddAlbumError),
    #[error(transparent)]
    UpdateDatabase(#[from] moosicbox_scan::output::UpdateDatabaseError),
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
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
            crate::library::get_library_album(db, &album.id.into(), ApiSource::Library).await?
        {
            albums.push(album);
        }
    }

    moosicbox_search::populate_global_search_index(
        &albums
            .clone()
            .into_iter()
            .map(Into::into)
            .map(|album: Album| album.as_data_values())
            .collect::<Vec<_>>(),
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
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    LibraryAlbum(#[from] LibraryAlbumError),
    #[error(transparent)]
    Album(#[from] moosicbox_music_api::AlbumError),
    #[error(transparent)]
    Tracks(#[from] moosicbox_music_api::TracksError),
    #[error(transparent)]
    RemoveAlbum(#[from] moosicbox_music_api::RemoveAlbumError),
    #[error(transparent)]
    DeleteFromIndex(#[from] DeleteFromIndexError),
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
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
        .filter(|track| match track.track_source {
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => album
                .album_sources
                .iter()
                .any(|x| x.source == ApiSource::Tidal),
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => album
                .album_sources
                .iter()
                .any(|x| x.source == ApiSource::Qobuz),
            _ => false,
        })
        .collect::<Vec<_>>();

    let track_ids = target_tracks
        .iter()
        .map(|t| t.id.clone())
        .collect::<Vec<_>>();

    log::debug!("Deleting track db items: {track_ids:?}");
    delete_session_playlist_tracks_by_track_id(
        db,
        Some(&track_ids.iter().map(Into::into).collect::<Vec<_>>()),
    )
    .await?;
    delete_track_sizes_by_track_id(
        db,
        Some(&track_ids.iter().map(Into::into).collect::<Vec<_>>()),
    )
    .await?;
    delete_tracks(
        db,
        Some(&track_ids.iter().map(Into::into).collect::<Vec<_>>()),
    )
    .await?;

    #[allow(unused_mut)]
    let mut album_field_updates: Vec<(&str, DatabaseValue)> = vec![];

    match api.source() {
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => {
            album_field_updates.push(("tidal_id", DatabaseValue::Null));
            album.album_sources.remove_source(ApiSource::Tidal);
        }
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => {
            album_field_updates.push(("qobuz_id", DatabaseValue::Null));
            album.album_sources.remove_source(ApiSource::Qobuz);
        }
        _ => {}
    }

    if !album_field_updates.is_empty() {
        db.update("albums")
            .where_eq("id", album.id)
            .values(album_field_updates)
            .execute(db)
            .await?;
    }

    moosicbox_library::cache::clear_cache();

    moosicbox_search::delete_from_global_search_index(
        &target_tracks
            .iter()
            .map(AsDeleteTerm::as_delete_term)
            .collect::<Vec<_>>(),
    )?;

    if has_local_tracks
        || match api.source() {
            #[cfg(feature = "tidal")]
            ApiSource::Tidal => album
                .album_sources
                .iter()
                .any(|x| x.source == ApiSource::Tidal),
            #[cfg(feature = "qobuz")]
            ApiSource::Qobuz => album
                .album_sources
                .iter()
                .any(|x| x.source == ApiSource::Qobuz),
            _ => false,
        }
    {
        log::debug!("Album has other sources, keeping LibraryAlbum");
        return Ok(album);
    }

    log::debug!("Deleting album db item: {}", album.id);
    db.delete("albums")
        .where_eq("id", album.id)
        .execute(db)
        .await?;

    {
        let album: Album = album.clone().into();

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
    Artist(#[from] moosicbox_music_api::ArtistError),
    #[error(transparent)]
    LibraryAlbum(#[from] LibraryAlbumError),
    #[error(transparent)]
    Album(#[from] moosicbox_music_api::AlbumError),
    #[error(transparent)]
    Albums(#[from] moosicbox_music_api::AlbumsError),
    #[error(transparent)]
    ArtistAlbums(#[from] moosicbox_music_api::ArtistAlbumsError),
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
        .map(Into::into);

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
