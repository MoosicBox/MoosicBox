use std::{
    cmp::Ordering,
    sync::{Arc, PoisonError},
};

use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::{
            delete_session_playlist_tracks_by_track_id_database,
            delete_track_sizes_by_track_id_database, delete_tracks_database, get_albums, DbError,
        },
        menu::GetAlbumError,
        models::{
            track_source_to_u8, Album, AlbumSort, AlbumSource, ApiSource, ApiTrack, LibraryAlbum,
            LibraryTrack, ToApi, TrackApiSource,
        },
    },
    types::AudioFormat,
};
use moosicbox_database::{query::*, Database, DatabaseError, DatabaseValue};
use moosicbox_music_api::{AlbumType, Id, LibraryAlbumError, MusicApi};
use moosicbox_scan::output::ScanOutput;
use moosicbox_search::{
    data::{AsDataValues, AsDeleteTerm},
    DeleteFromIndexError, PopulateIndexError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<AlbumSort>,
    pub filters: AlbumFilters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFilters {
    pub name: Option<String>,
    pub artist: Option<String>,
    pub search: Option<String>,
    pub artist_id: Option<i32>,
    pub tidal_artist_id: Option<u64>,
    pub qobuz_artist_id: Option<u64>,
}

pub fn filter_albums(albums: Vec<LibraryAlbum>, request: &AlbumsRequest) -> Vec<LibraryAlbum> {
    albums
        .into_iter()
        .filter(|album| {
            !request
                .filters
                .artist_id
                .as_ref()
                .is_some_and(|id| album.artist_id != *id)
        })
        .filter(|album| {
            !request
                .filters
                .tidal_artist_id
                .as_ref()
                .is_some_and(|id| !album.tidal_artist_id.is_some_and(|x| x == *id))
        })
        .filter(|album| {
            !request
                .filters
                .qobuz_artist_id
                .as_ref()
                .is_some_and(|id| !album.qobuz_artist_id.is_some_and(|x| x == *id))
        })
        .filter(|album| {
            !request
                .sources
                .as_ref()
                .is_some_and(|s| !s.contains(&album.source))
        })
        .filter(|album| {
            !request
                .filters
                .name
                .as_ref()
                .is_some_and(|s| !album.title.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request
                .filters
                .artist
                .as_ref()
                .is_some_and(|s| !&album.artist.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request.filters.search.as_ref().is_some_and(|s| {
                !(album.title.to_lowercase().contains(s) || album.artist.to_lowercase().contains(s))
            })
        })
        .collect()
}

pub fn sort_albums(mut albums: Vec<LibraryAlbum>, request: &AlbumsRequest) -> Vec<LibraryAlbum> {
    match request.sort {
        Some(AlbumSort::ArtistAsc) => albums.sort_by(|a, b| a.artist.cmp(&b.artist)),
        Some(AlbumSort::NameAsc) => albums.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(AlbumSort::ArtistDesc) => albums.sort_by(|a, b| b.artist.cmp(&a.artist)),
        Some(AlbumSort::NameDesc) => albums.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(AlbumSort::ArtistAsc) => {
            albums.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
        }
        Some(AlbumSort::NameAsc) => {
            albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
        Some(AlbumSort::ArtistDesc) => {
            albums.sort_by(|a, b| b.artist.to_lowercase().cmp(&a.artist.to_lowercase()))
        }
        Some(AlbumSort::NameDesc) => {
            albums.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()))
        }
        Some(AlbumSort::ReleaseDateAsc) => albums.sort_by(|a, b| {
            if a.date_released.is_none() {
                return Ordering::Greater;
            }
            if b.date_released.is_none() {
                return Ordering::Less;
            }

            a.clone().date_released.cmp(&b.clone().date_released)
        }),
        Some(AlbumSort::ReleaseDateDesc) => albums.sort_by(|a, b| {
            if a.date_released.is_none() {
                return Ordering::Greater;
            }
            if b.date_released.is_none() {
                return Ordering::Less;
            }

            b.clone().date_released.cmp(&a.clone().date_released)
        }),
        Some(AlbumSort::DateAddedAsc) => {
            albums.sort_by(|a, b| a.clone().date_added.cmp(&b.clone().date_added))
        }
        Some(AlbumSort::DateAddedDesc) => {
            albums.sort_by(|b, a| a.clone().date_added.cmp(&b.clone().date_added))
        }
        None => (),
    }

    albums
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
}

pub async fn get_all_albums(
    data: &AppState,
    request: &AlbumsRequest,
) -> Result<Vec<LibraryAlbum>, GetAlbumsError> {
    let albums = get_albums(
        &data
            .db
            .as_ref()
            .ok_or(GetAlbumsError::NoDb)?
            .library
            .lock()
            .unwrap()
            .inner,
    )?;

    Ok(sort_albums(filter_albums(albums, request), request))
}

#[derive(Debug, Error)]
pub enum GetAlbumTracksError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Json(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
}

impl<T> From<PoisonError<T>> for GetAlbumTracksError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub fn get_album_tracks(
    album_id: i32,
    data: &AppState,
) -> Result<Vec<LibraryTrack>, GetAlbumTracksError> {
    let library = data
        .db
        .as_ref()
        .ok_or(GetAlbumTracksError::NoDb)?
        .library
        .lock()?;

    Ok(moosicbox_core::sqlite::db::get_album_tracks(
        &library.inner,
        album_id,
    )?)
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
    GetAlbumTracks(#[from] GetAlbumTracksError),
}

pub fn get_album_versions(
    album_id: i32,
    data: &AppState,
) -> Result<Vec<AlbumVersion>, GetAlbumVersionsError> {
    let tracks = get_album_tracks(album_id, data)?;
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
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    LibraryAlbum(#[from] LibraryAlbumError),
    #[error(transparent)]
    Album(#[from] moosicbox_music_api::AlbumError),
    #[error(transparent)]
    AddAlbum(#[from] moosicbox_music_api::AddAlbumError),
    #[error(transparent)]
    TidalScan(#[from] moosicbox_scan::tidal::ScanError),
    #[error(transparent)]
    QobuzScan(#[from] moosicbox_scan::qobuz::ScanError),
    #[error(transparent)]
    UpdateDatabase(#[from] moosicbox_scan::output::UpdateDatabaseError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    #[error("No album")]
    NoAlbum,
    #[error("Invalid album_id type")]
    InvalidAlbumIdType,
}

pub async fn add_album(
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
    api: &dyn MusicApi,
) -> Result<LibraryAlbum, AddAlbumError> {
    log::debug!(
        "Adding album to library album_id={album_id:?} source={}",
        api.source()
    );

    if let Some(album) = api.library_album(album_id).await? {
        log::debug!("Album album_id={album_id:?} already added: album={album:?}");
        return Ok(album);
    }

    let output = Arc::new(RwLock::new(ScanOutput::new()));

    let album = api.album(album_id).await?.ok_or(AddAlbumError::NoAlbum)?;

    api.add_album(album_id).await?;

    match album {
        Album::Tidal(album) => {
            moosicbox_scan::tidal::scan_albums(&[album], 1, db.clone(), output.clone(), None)
                .await?;
        }
        Album::Qobuz(album) => {
            moosicbox_scan::qobuz::scan_albums(&[album], 1, db.clone(), output.clone(), None)
                .await?;
        }
        _ => {}
    }

    let output = output.read().await;
    let results = output.update_database(db.clone()).await?;

    moosicbox_core::cache::clear_cache();

    moosicbox_search::populate_global_search_index(
        results
            .artists
            .iter()
            .map(|artist| artist.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let mut albums = vec![];

    for album in &results.albums {
        if let Some(album) = moosicbox_core::sqlite::db::get_album_database(
            &db,
            "id",
            DatabaseValue::UNumber(album.id as u64),
        )
        .await?
        {
            albums.push(album);
        }
    }

    moosicbox_search::populate_global_search_index(
        albums
            .iter()
            .map(|album| album.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let tracks = moosicbox_core::sqlite::db::get_tracks_database(
        &db,
        Some(
            &results
                .tracks
                .iter()
                .map(|t| t.id as u64)
                .collect::<Vec<_>>(),
        ),
    )
    .await?;
    moosicbox_search::populate_global_search_index(
        tracks
            .iter()
            .map(|track| track.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    if let Some(album) = albums.into_iter().next() {
        return Ok(album);
    }

    api.library_album(album_id)
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
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    LibraryAlbum(#[from] LibraryAlbumError),
    #[error(transparent)]
    Album(#[from] moosicbox_music_api::AlbumError),
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
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
    api: &dyn MusicApi,
) -> Result<LibraryAlbum, RemoveAlbumError> {
    log::debug!(
        "Removing album from library album_id={album_id:?} source={}",
        api.source()
    );

    let mut album = api
        .library_album(album_id)
        .await?
        .ok_or(RemoveAlbumError::NoAlbum)?;

    log::debug!("Removing album from library album={album:?}");

    if let Err(err) = api.remove_album(album_id).await {
        log::error!("Failed to remove album from MusicApi: {err:?}");
    }

    let tracks =
        moosicbox_core::sqlite::db::get_album_tracks_database(&db, album.id as u64).await?;

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

    let track_ids = target_tracks.iter().map(|t| t.id).collect::<Vec<_>>();

    log::debug!("Deleting track db items: {track_ids:?}");
    delete_session_playlist_tracks_by_track_id_database(&db, Some(&track_ids)).await?;
    delete_track_sizes_by_track_id_database(&db, Some(&track_ids)).await?;
    delete_tracks_database(&db, Some(&track_ids)).await?;

    let mut album_field_updates = vec![];

    match api.source() {
        ApiSource::Tidal => {
            album_field_updates.push(("tidal_id", DatabaseValue::NumberOpt(None)));
            album.tidal_id = None;
        }
        ApiSource::Qobuz => {
            album_field_updates.push(("qobuz_id", DatabaseValue::NumberOpt(None)));
            album.qobuz_id = None;
        }
        _ => {}
    }

    if !album_field_updates.is_empty() {
        db.update_and_get_row(
            "albums",
            DatabaseValue::Number(album.id as i64),
            &album_field_updates,
        )
        .await?;
    }

    moosicbox_core::cache::clear_cache();

    moosicbox_search::delete_from_global_search_index(
        target_tracks
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
    db.delete(
        "albums",
        Some(&[where_eq("id", DatabaseValue::Number(album.id as i64))]),
    )
    .await?;

    moosicbox_search::delete_from_global_search_index(vec![album.as_delete_term()])?;

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
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
    api: &dyn MusicApi,
) -> Result<LibraryAlbum, ReFavoriteAlbumError> {
    log::debug!(
        "Re-favoriting album from library album_id={album_id:?} source={}",
        api.source()
    );

    let favorite_albums = api
        .albums(None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;

    let album = favorite_albums
        .iter()
        .find(|album| &Into::<Id>::into(album.id()) == album_id)
        .ok_or(ReFavoriteAlbumError::NoAlbum)?;

    let artist = api
        .artist(&album.artist_id().into())
        .await?
        .ok_or(ReFavoriteAlbumError::NoArtist)?;

    let new_album_id = api
        .artist_albums(&artist.id().into(), AlbumType::All, None, None, None, None)
        .await?
        .with_rest_of_items()
        .await?
        .iter()
        .find(|x| {
            x.artist_id() == album.artist_id()
                && x.title().to_lowercase().trim() == album.title().to_lowercase().trim()
        })
        .map(|x| x.id());

    let new_album_id = if let Some(album_id) = new_album_id {
        album_id
    } else {
        log::debug!("No corresponding album to re-favorite album_id={album_id}");
        return Err(ReFavoriteAlbumError::NoAlbum);
    };

    log::debug!("Re-favoriting with ids album_id={album_id} new_album_id={new_album_id:?}");

    remove_album(db.clone(), album_id, api).await?;
    let album = add_album(db, &new_album_id.into(), api).await?;

    Ok(album)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filter_albums_empty_albums_returns_empty_albums() {
        let albums = vec![];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![]);
    }

    #[test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        let local = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let tidal = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Tidal,
            ..Default::default()
        };
        let qobuz = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Qobuz,
            ..Default::default()
        };
        let albums = vec![local.clone(), tidal, qobuz];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: Some(vec![AlbumSource::Local]),
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![local]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match() {
        let bob = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist() {
        let bob = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_and_searches_across_properties() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob.clone(), sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
                },
            },
        );
        assert_eq!(result, vec![bob, test]);
    }
}
