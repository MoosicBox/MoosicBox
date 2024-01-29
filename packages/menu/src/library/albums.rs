use std::{
    cmp::Ordering,
    sync::{Arc, PoisonError},
};

use moosicbox_core::{
    app::{AppState, Db},
    sqlite::{
        db::{
            delete, delete_session_playlist_tracks_by_track_id, delete_track_sizes_by_track_id,
            delete_tracks, get_albums, DbError, SqliteValue,
        },
        menu::{get_album, GetAlbumError},
        models::{
            track_source_to_u8, Album, AlbumSort, AlbumSource, ApiTrack, LibraryTrack, ToApi,
            TrackSource,
        },
    },
    types::AudioFormat,
};
use moosicbox_qobuz::{QobuzAddFavoriteAlbumError, QobuzAlbumError, QobuzRemoveFavoriteAlbumError};
use moosicbox_scan::output::ScanOutput;
use moosicbox_search::{
    data::{AsDataValues, AsDeleteTerm},
    DeleteFromIndexError, PopulateIndexError,
};
use moosicbox_tidal::{TidalAddFavoriteAlbumError, TidalAlbumError, TidalRemoveFavoriteAlbumError};
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

pub fn filter_albums(albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
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

pub fn sort_albums(mut albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
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
) -> Result<Vec<Album>, GetAlbumsError> {
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
    pub source: TrackSource,
}

impl ToApi<ApiAlbumVersion> for AlbumVersion {
    fn to_api(&self) -> ApiAlbumVersion {
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
    pub source: TrackSource,
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
    TidalAddFavoriteAlbum(#[from] TidalAddFavoriteAlbumError),
    #[error(transparent)]
    TidalAlbum(#[from] TidalAlbumError),
    #[error(transparent)]
    TidalScan(#[from] moosicbox_scan::tidal::ScanError),
    #[error(transparent)]
    QobuzAddFavoriteAlbum(#[from] QobuzAddFavoriteAlbumError),
    #[error(transparent)]
    QobuzAlbum(#[from] QobuzAlbumError),
    #[error(transparent)]
    QobuzScan(#[from] moosicbox_scan::qobuz::ScanError),
    #[error(transparent)]
    UpdateDatabase(#[from] moosicbox_scan::output::UpdateDatabaseError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
}

pub async fn add_album(
    db: &Db,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<String>,
) -> Result<(), AddAlbumError> {
    match get_album(None, tidal_album_id, qobuz_album_id.clone(), db).await {
        Ok(album) => {
            log::debug!("Album tidal_album_id={tidal_album_id:?} qobuz_album_id={qobuz_album_id:?} already added: album={album:?}");
            return Ok(());
        }
        Err(GetAlbumError::AlbumNotFound { .. }) => {}
        Err(err) => {
            return Err(AddAlbumError::GetAlbum(err));
        }
    }

    let output = Arc::new(RwLock::new(ScanOutput::new()));

    if let Some(album_id) = tidal_album_id {
        let album = moosicbox_tidal::album(db, album_id, None, None, None, None).await?;
        moosicbox_tidal::add_favorite_album(db, album_id, None, None, None, None, None).await?;
        moosicbox_scan::tidal::scan_albums(vec![album], 1, db, output.clone(), None).await?;
    }
    if let Some(album_id) = &qobuz_album_id {
        let album = moosicbox_qobuz::album(db, album_id, None, None).await?;
        moosicbox_qobuz::add_favorite_album(db, album_id, None, None).await?;
        moosicbox_scan::qobuz::scan_albums(vec![album], 1, db, output.clone(), None).await?;
    }

    let output = output.read().await;
    let results = output.update_database(db).await?;

    moosicbox_core::cache::clear_cache();

    moosicbox_search::populate_global_search_index(
        results
            .artists
            .iter()
            .map(|artist| artist.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let albums = results
        .albums
        .iter()
        .map(|album| {
            moosicbox_core::sqlite::db::get_album(
                &db.library.lock().as_ref().unwrap().inner,
                album.id,
            )
        })
        .filter_map(|album| album.ok())
        .map(|album| album.unwrap())
        .collect::<Vec<_>>();
    moosicbox_search::populate_global_search_index(
        albums
            .iter()
            .map(|album| album.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    let tracks = moosicbox_core::sqlite::db::get_tracks(
        &db.library.lock().as_ref().unwrap().inner,
        Some(&results.tracks.iter().map(|t| t.id).collect::<Vec<_>>()),
    )?;
    moosicbox_search::populate_global_search_index(
        tracks
            .iter()
            .map(|track| track.as_data_values())
            .collect::<Vec<_>>(),
        false,
    )?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum RemoveAlbumError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    TidalRemoveFavoriteAlbum(#[from] TidalRemoveFavoriteAlbumError),
    #[error(transparent)]
    QobuzRemoveFavoriteAlbum(#[from] QobuzRemoveFavoriteAlbumError),
    #[error(transparent)]
    DeleteFromIndex(#[from] DeleteFromIndexError),
}

pub async fn remove_album(
    db: &Db,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<String>,
) -> Result<(), RemoveAlbumError> {
    log::debug!("Removing album from library tidal_album_id={tidal_album_id:?} qobuz_album_id={qobuz_album_id:?}");

    let album = match get_album(None, tidal_album_id, qobuz_album_id.clone(), db).await {
        Ok(album) => album,
        Err(GetAlbumError::AlbumNotFound { .. }) => {
            log::debug!("Album tidal_album_id={tidal_album_id:?} qobuz_album_id={qobuz_album_id:?} already removed");
            return Ok(());
        }
        Err(err) => {
            return Err(RemoveAlbumError::GetAlbum(err));
        }
    };

    if let Some(album_id) = tidal_album_id {
        moosicbox_tidal::remove_favorite_album(db, album_id, None, None, None, None, None).await?;
    }
    if let Some(album_id) = qobuz_album_id {
        moosicbox_qobuz::remove_favorite_album(db, &album_id, None, None).await?;
    }

    let tracks = moosicbox_core::sqlite::db::get_album_tracks(
        &db.library.lock().as_ref().unwrap().inner,
        album.id,
    )?;

    let track_ids = tracks.iter().map(|t| t.id).collect::<Vec<_>>();

    log::debug!("Deleting track db items: {track_ids:?}");
    delete_session_playlist_tracks_by_track_id(
        &db.library.lock().as_ref().unwrap().inner,
        Some(&track_ids),
    )?;
    delete_track_sizes_by_track_id(&db.library.lock().as_ref().unwrap().inner, Some(&track_ids))?;
    delete_tracks(&db.library.lock().as_ref().unwrap().inner, Some(&track_ids))?;

    log::debug!("Deleting album db item: {}", album.id);
    delete::<Album>(
        &db.library.lock().as_ref().unwrap().inner,
        "albums",
        &vec![("id", SqliteValue::Number(album.id as i64))],
    )?;

    moosicbox_core::cache::clear_cache();

    moosicbox_search::delete_from_global_search_index(vec![album.as_delete_term()])?;
    moosicbox_search::delete_from_global_search_index(
        tracks
            .iter()
            .map(|track| track.as_delete_term())
            .collect::<Vec<_>>(),
    )?;

    Ok(())
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
        let local = Album {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let tidal = Album {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Tidal,
            ..Default::default()
        };
        let qobuz = Album {
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
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
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
