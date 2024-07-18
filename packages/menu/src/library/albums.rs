use std::sync::{Arc, PoisonError};

use async_recursion::async_recursion;
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Album, ApiSource, Artist, Id, ToApi, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_database::{query::*, Database, DatabaseError, DatabaseValue};
use moosicbox_library::{
    db::{delete_track_sizes_by_track_id, delete_tracks},
    models::{track_source_to_u8, ApiTrack, LibraryAlbum, LibraryTrack},
    LibraryAlbumTracksError, LibraryFavoriteAlbumsError, LibraryMusicApi,
};
use moosicbox_music_api::{AlbumType, AlbumsRequest, LibraryAlbumError, MusicApi, TracksError};
use moosicbox_paging::{PagingRequest, PagingResponse, PagingResult};
use moosicbox_scan::{music_api::ScanError, output::ScanOutput};
use moosicbox_search::{
    data::{AsDataValues, AsDeleteTerm},
    DeleteFromIndexError, PopulateIndexError,
};
use moosicbox_session::db::delete_session_playlist_tracks_by_track_id;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use super::GetAlbumError;

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    LibraryFavoriteAlbums(#[from] LibraryFavoriteAlbumsError),
}

#[async_recursion]
pub async fn get_all_albums(
    library_api: Arc<LibraryMusicApi>,
    request: &AlbumsRequest,
) -> PagingResult<LibraryAlbum, GetAlbumsError> {
    let items = library_api
        .library_albums(request)
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    let total = items.len() as u32;

    Ok(PagingResponse {
        page: moosicbox_paging::Page::WithTotal {
            items,
            offset: 0,
            limit: total,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new({
            let request = request.clone();

            move |offset, limit| {
                let mut request = request.clone();
                request.page = Some(PagingRequest { offset, limit });
                let library_api = library_api.clone();

                Box::pin(async move { get_all_albums(library_api, &request).await })
            }
        }))),
    })
}

#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Json(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

#[derive(Clone)]
pub struct AlbumVersion {
    pub tracks: Vec<LibraryTrack>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

impl ToApi<ApiAlbumVersion> for AlbumVersion {
    fn to_api(self) -> ApiAlbumVersion {
        ApiAlbumVersion {
            tracks: self.tracks.iter().map(|track| track.to_api()).collect(),
            format: self.format,
            bit_depth: self.bit_depth,
            sample_rate: self.sample_rate,
            channels: self.channels,
            source: self.source,
        }
    }
}

pub fn sort_album_versions(versions: &mut [AlbumVersion]) {
    versions.sort_by(|a, b| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a, b| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });
    versions.sort_by(|a, b| track_source_to_u8(a.source).cmp(&track_source_to_u8(b.source)));
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersion {
    pub tracks: Vec<ApiTrack>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

#[derive(Debug, Error)]
pub enum GetAlbumVersionsError {
    #[error(transparent)]
    Tracks(#[from] TracksError),
    #[error(transparent)]
    LibraryAlbumTracks(#[from] LibraryAlbumTracksError),
}

pub async fn get_album_versions(
    library_api: &LibraryMusicApi,
    album_id: &Id,
) -> Result<Vec<AlbumVersion>, GetAlbumVersionsError> {
    let tracks = library_api
        .library_album_tracks(album_id, None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;
    log::trace!("Got {} album id={album_id} tracks", tracks.len());

    let mut versions = vec![];

    for track in tracks {
        if versions.is_empty() {
            log::trace!("No versions exist yet. Creating first version");
            versions.push(AlbumVersion {
                tracks: vec![track.clone()],
                format: track.format,
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                channels: track.channels,
                source: track.source,
            });
            continue;
        }

        if let Some(existing_version) = versions.iter_mut().find(|v| {
            v.sample_rate == track.sample_rate
                && v.bit_depth == track.bit_depth
                && v.tracks[0].directory() == track.directory()
                && v.source == track.source
        }) {
            log::trace!("Adding track to existing version");
            existing_version.tracks.push(track);
        } else {
            log::trace!("Adding track to new version");
            versions.push(AlbumVersion {
                tracks: vec![track.clone()],
                format: track.format,
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                channels: track.channels,
                source: track.source,
            });
            continue;
        }
    }

    sort_album_versions(&mut versions);

    Ok(versions)
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

pub async fn add_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
) -> Result<LibraryAlbum, AddAlbumError> {
    log::debug!(
        "Adding album to library album_id={album_id:?} source={}",
        api.source()
    );

    if let Some(album) = library_api.album(album_id).await? {
        log::debug!("Album album_id={album_id:?} already added: album={album:?}");
        return Ok(album.into());
    }

    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let album = api.album(album_id).await?.ok_or(AddAlbumError::NoAlbum)?;

    api.add_album(album_id).await?;

    moosicbox_scan::music_api::scan_albums(api, &[album], 1, output.clone(), None).await?;

    let output = output.read().await;
    let results = output.update_database(&**db).await?;

    moosicbox_library::cache::clear_cache();

    moosicbox_search::populate_global_search_index(
        &results
            .artists
            .clone()
            .into_iter()
            .map(|x| x.into())
            .map(|artist: Artist| artist.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let mut albums = vec![];

    for album in &results.albums {
        if let Some(album) =
            crate::library::get_album(&**db, &album.id.into(), ApiSource::Library).await?
        {
            albums.push(album);
        }
    }

    moosicbox_search::populate_global_search_index(
        &albums
            .clone()
            .into_iter()
            .map(|x| x.into())
            .map(|album: Album| album.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let tracks = api
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
    moosicbox_search::populate_global_search_index(
        &tracks
            .iter()
            .map(|track| track.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    if let Some(album) = albums.into_iter().next() {
        return Ok(album);
    }

    library_api
        .library_album(album_id)
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

pub async fn remove_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: &dyn Database,
    album_id: &Id,
) -> Result<LibraryAlbum, RemoveAlbumError> {
    log::debug!(
        "Removing album from library album_id={album_id:?} source={}",
        api.source()
    );

    let mut album = library_api
        .library_album(album_id)
        .await?
        .ok_or(RemoveAlbumError::NoAlbum)?;

    log::debug!("Removing album from library album={album:?}");

    if let Err(err) = api.remove_album(album_id).await {
        log::error!("Failed to remove album from MusicApi: {err:?}");
    }

    let tracks = library_api
        .album_tracks(album_id, None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    let has_local_tracks = tracks
        .iter()
        .any(|track| track.source == TrackApiSource::Local);

    let target_tracks = tracks
        .into_iter()
        .filter(|track| match track.source {
            TrackApiSource::Tidal => album.tidal_id.is_some(),
            TrackApiSource::Qobuz => album.qobuz_id.is_some(),
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
        Some(&track_ids.iter().map(|x| x.into()).collect::<Vec<_>>()),
    )
    .await?;
    delete_track_sizes_by_track_id(
        db,
        Some(&track_ids.iter().map(|x| x.into()).collect::<Vec<_>>()),
    )
    .await?;
    delete_tracks(
        db,
        Some(&track_ids.iter().map(|x| x.into()).collect::<Vec<_>>()),
    )
    .await?;

    let mut album_field_updates = vec![];

    match api.source() {
        ApiSource::Tidal => {
            album_field_updates.push(("tidal_id", DatabaseValue::Null));
            album.tidal_id = None;
        }
        ApiSource::Qobuz => {
            album_field_updates.push(("qobuz_id", DatabaseValue::Null));
            album.qobuz_id = None;
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
            .map(|track| track.as_delete_term())
            .collect::<Vec<_>>(),
    )?;

    if has_local_tracks
        || match api.source() {
            ApiSource::Tidal => album.tidal_id.is_some(),
            ApiSource::Qobuz => album.qobuz_id.is_some(),
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
    #[error("No artist")]
    NoArtist,
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

pub async fn refavorite_album(
    api: &dyn MusicApi,
    library_api: &LibraryMusicApi,
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
) -> Result<LibraryAlbum, ReFavoriteAlbumError> {
    log::debug!(
        "Re-favoriting album from library album_id={album_id:?} source={}",
        api.source()
    );

    let favorite_albums = api
        .albums(&AlbumsRequest::default())
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    let album = favorite_albums
        .iter()
        .find(|album| &album.id == album_id)
        .ok_or(ReFavoriteAlbumError::NoAlbum)?;

    let artist = api
        .artist(&album.artist_id)
        .await?
        .ok_or(ReFavoriteAlbumError::NoArtist)?;

    let new_album_id = api
        .artist_albums(&artist.id, AlbumType::All, None, None, None, None)
        .await?
        .with_rest_of_items()
        .await?
        .iter()
        .find(|x| {
            x.artist_id == album.artist_id
                && x.title.to_lowercase().trim() == album.title.to_lowercase().trim()
        })
        .map(|x| x.id.clone());

    let new_album_id = if let Some(album_id) = new_album_id {
        album_id
    } else {
        log::debug!("No corresponding album to re-favorite album_id={album_id}");
        return Err(ReFavoriteAlbumError::NoAlbum);
    };

    log::debug!("Re-favoriting with ids album_id={album_id} new_album_id={new_album_id:?}");

    remove_album(api, library_api, &**db, album_id).await?;
    let album = add_album(api, library_api, db, &new_album_id).await?;

    Ok(album)
}
