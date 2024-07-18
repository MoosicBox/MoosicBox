use std::str::FromStr;

use actix_web::{
    delete,
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{
    app::AppState,
    integer_range::ParseIntegersError,
    sqlite::models::{AlbumSort, AlbumSource, ArtistSort, ToApi},
};
use moosicbox_core::{
    integer_range::parse_integer_ranges_to_ids,
    sqlite::models::{ApiSource, Id},
};
use moosicbox_library::{
    db::{get_album_tracks, get_tracks},
    models::{ApiAlbum, ApiArtist, ApiTrack},
    LibraryMusicApiState,
};
use moosicbox_music_api::{AlbumFilters, AlbumsRequest, MusicApiState};
use moosicbox_paging::{Page, PagingRequest};
use serde::Deserialize;
use thiserror::Error;

use crate::library::{
    albums::{add_album, get_album_versions, refavorite_album, remove_album, ApiAlbumVersion},
    artists::{get_all_artists, ArtistFilters, ArtistsRequest},
    get_album, get_artist, get_artist_albums, GetArtistError,
};

fn album_id_for_source(id: &str, source: ApiSource) -> Result<Id, actix_web::Error> {
    Ok(match source {
        ApiSource::Tidal => id
            .parse::<u64>()
            .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?
            .into(),

        ApiSource::Qobuz => id.to_string().into(),
        ApiSource::Yt => id.to_string().into(),
        ApiSource::Library => id
            .parse::<i32>()
            .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?
            .into(),
    })
}

#[derive(Debug, Error)]
pub enum MenuError {
    #[error(transparent)]
    BadRequest(#[from] actix_web::Error),
    #[error("Internal server error: {error:?}")]
    InternalServerError { error: String },
    #[error("Not Found Error: {error:?}")]
    NotFound { error: String },
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistsQuery {
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    search: Option<String>,
}

#[get("/artists")]
pub async fn get_artists_endpoint(
    query: web::Query<GetArtistsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiArtist>>> {
    let request = ArtistsRequest {
        sources: query
            .sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(|s| s.trim())
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: query
            .sort
            .as_ref()
            .map(|sort| {
                ArtistSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
        filters: ArtistFilters {
            name: query.name.clone().map(|s| s.to_lowercase()),
            search: query.search.clone().map(|s| s.to_lowercase()),
        },
    };

    Ok(Json(
        get_all_artists(&data, &request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch artists: {e:?}")))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    artist: Option<String>,
    search: Option<String>,
    artist_id: Option<i32>,
    tidal_artist_id: Option<u64>,
    qobuz_artist_id: Option<u64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    library_api: web::Data<LibraryMusicApiState>,
) -> Result<Json<Page<ApiAlbum>>> {
    let request = AlbumsRequest {
        page: if query.offset.is_some() || query.limit.is_some() {
            Some(PagingRequest {
                offset: query.offset.unwrap_or(0),
                limit: query.limit.unwrap_or(100),
            })
        } else {
            None
        },
        sources: query
            .sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(|s| s.trim())
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: query
            .sort
            .as_ref()
            .map(|sort| {
                AlbumSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
        filters: Some(AlbumFilters {
            name: query.name.clone().map(|s| s.to_lowercase()),
            artist: query.artist.clone().map(|s| s.to_lowercase()),
            search: query.search.clone().map(|s| s.to_lowercase()),
            artist_id: query.artist_id.map(|x| x.into()),
            tidal_artist_id: query.tidal_artist_id.map(|x| x.into()),
            qobuz_artist_id: query.qobuz_artist_id.map(|x| x.into()),
        }),
    };

    Ok(Json(
        library_api
            .library_albums(&request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e}")))?
            .to_api()
            .into(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTracksQuery {
    track_ids: String,
}

#[get("/tracks")]
pub async fn get_tracks_endpoint(
    query: web::Query<GetTracksQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiTrack>>> {
    let ids = parse_integer_ranges_to_ids(&query.track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    Ok(Json(
        get_tracks(&**data.database, Some(&ids))
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch tracks"))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumTracksQuery {
    album_id: i32,
}

#[get("/album/tracks")]
pub async fn get_album_tracks_endpoint(
    query: web::Query<GetAlbumTracksQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiTrack>>> {
    Ok(Json(
        get_album_tracks(&**data.database, &query.album_id.into())
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch tracks"))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumVersionsQuery {
    album_id: i32,
}

#[get("/album/versions")]
pub async fn get_album_versions_endpoint(
    query: web::Query<GetAlbumVersionsQuery>,
    library_api: web::Data<LibraryMusicApiState>,
) -> Result<Json<Vec<ApiAlbumVersion>>> {
    Ok(Json(
        get_album_versions(&library_api, &query.album_id.into())
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch album versions"))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistAlbumsQuery {
    artist_id: i32,
}

#[get("/artist/albums")]
pub async fn get_artist_albums_endpoint(
    query: web::Query<GetArtistAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiAlbum>>> {
    Ok(Json(
        get_artist_albums(&query.artist_id.into(), &data)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch albums"))?
            .iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

impl From<GetArtistError> for actix_web::Error {
    fn from(e: GetArtistError) -> Self {
        match e {
            GetArtistError::ArtistNotFound(_) => {
                ErrorNotFound(format!("Failed to fetch artist: {e:?}"))
            }
            GetArtistError::AlbumArtistNotFound(_) => {
                ErrorNotFound(format!("Failed to fetch artist: {e:?}"))
            }
            GetArtistError::UnknownSource { .. }
            | GetArtistError::PoisonError
            | GetArtistError::SqliteError(_)
            | GetArtistError::DbError(_) => {
                ErrorInternalServerError(format!("Failed to fetch artist: {e:?}"))
            }
            GetArtistError::InvalidRequest => {
                ErrorBadRequest(format!("Failed to fetch artist: {e:?}"))
            }
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistQuery {
    artist_id: Option<u64>,
    tidal_artist_id: Option<u64>,
    qobuz_artist_id: Option<u64>,
    album_id: Option<u64>,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<u64>,
}

#[get("/artist")]
pub async fn get_artist_endpoint(
    query: web::Query<GetArtistQuery>,
    data: web::Data<AppState>,
) -> Result<Json<ApiArtist>> {
    Ok(Json(
        get_artist(
            query.artist_id,
            query.tidal_artist_id,
            query.qobuz_artist_id,
            query.album_id,
            query.tidal_album_id,
            query.qobuz_album_id,
            &data,
        )
        .await?
        .to_api(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumQuery {
    album_id: Option<u64>,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<String>,
}

#[get("/album")]
pub async fn get_album_endpoint(
    query: web::Query<GetAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<ApiAlbum>> {
    let (id, source) = if let Some(id) = query.album_id {
        (id.into(), ApiSource::Library)
    } else if let Some(id) = query.tidal_album_id {
        (id.into(), ApiSource::Tidal)
    } else if let Some(id) = &query.qobuz_album_id {
        (id.into(), ApiSource::Qobuz)
    } else {
        return Err(ErrorNotFound("Album not found"));
    };

    Ok(Json(
        get_album(&**data.database, &id, source)
            .await?
            .ok_or(ErrorNotFound("Album not found"))?
            .to_api(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[post("/album")]
pub async fn add_album_endpoint(
    query: web::Query<AddAlbumQuery>,
    data: web::Data<AppState>,
    library_api: web::Data<LibraryMusicApiState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        add_album(
            &**api_state
                .apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            data.database.clone(),
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add album: {e:?}")))?
        .to_api(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[delete("/album")]
pub async fn remove_album_endpoint(
    query: web::Query<RemoveAlbumQuery>,
    data: web::Data<AppState>,
    library_api: web::Data<LibraryMusicApiState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        remove_album(
            &**api_state
                .apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            &**data.database,
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove album: {e:?}")))?
        .to_api(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReFavoriteAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[post("/album/re-favorite")]
pub async fn refavorite_album_endpoint(
    query: web::Query<ReFavoriteAlbumQuery>,
    data: web::Data<AppState>,
    library_api: web::Data<LibraryMusicApiState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        refavorite_album(
            &**api_state
                .apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            data.database.clone(),
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to re-favorite album: {e:?}")))?
        .to_api(),
    ))
}
