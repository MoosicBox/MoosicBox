use std::{str::FromStr, sync::Arc};

use actix_web::{
    delete,
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::sqlite::{
    menu::get_artist_albums,
    models::{AlbumId, ApiSource},
};
use moosicbox_core::{
    app::AppState,
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::{
        db::get_tracks,
        menu::{get_album, get_artist},
        models::{
            AlbumSort, AlbumSource, ApiAlbum, ApiArtist, ApiTrack, ArtistSort, LibraryAlbum, ToApi,
        },
    },
};
use moosicbox_database::Database;
use moosicbox_music_api::MusicApi;
use moosicbox_qobuz::QobuzMusicApi;
use moosicbox_tidal::TidalMusicApi;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::library::{
    albums::{
        add_album, get_album_tracks, get_album_versions, get_all_albums, refavorite_album,
        remove_album, AlbumFilters, AlbumsRequest, ApiAlbumVersion,
    },
    artists::{get_all_artists, ArtistFilters, ArtistsRequest},
};

fn album_id_for_source(id: &str, source: ApiSource) -> Result<AlbumId, actix_web::Error> {
    Ok(match source {
        ApiSource::Tidal => AlbumId::Tidal(
            id.parse::<u64>()
                .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?,
        ),
        ApiSource::Qobuz => AlbumId::Qobuz(id.to_string()),
        ApiSource::Library => AlbumId::Library(
            id.parse::<i32>()
                .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?,
        ),
    })
}

fn music_api_from_source(db: Arc<Box<dyn Database>>, source: ApiSource) -> Box<dyn MusicApi> {
    match source {
        ApiSource::Tidal => Box::new(TidalMusicApi::new(db.clone())),
        ApiSource::Qobuz => Box::new(QobuzMusicApi::new(db.clone())),
        ApiSource::Library => unimplemented!(),
    }
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

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum MenuResponse {
    Albums(Vec<LibraryAlbum>),
    Error(Value),
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
}

#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiAlbum>>> {
    let request = AlbumsRequest {
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
        filters: AlbumFilters {
            name: query.name.clone().map(|s| s.to_lowercase()),
            artist: query.artist.clone().map(|s| s.to_lowercase()),
            search: query.search.clone().map(|s| s.to_lowercase()),
            artist_id: query.artist_id,
            tidal_artist_id: query.tidal_artist_id,
            qobuz_artist_id: query.qobuz_artist_id,
        },
    };

    Ok(Json(
        get_all_albums(&data, &request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e}")))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
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
    let ids = parse_integer_ranges(&query.track_ids)
        .map_err(|e| match e {
            ParseIntegersError::ParseId(id) => {
                ErrorBadRequest(format!("Could not parse trackId '{id}'"))
            }
            ParseIntegersError::UnmatchedRange(range) => {
                ErrorBadRequest(format!("Unmatched range '{range}'"))
            }
            ParseIntegersError::RangeTooLarge(range) => {
                ErrorBadRequest(format!("Range too large '{range}'"))
            }
        })?
        .into_iter()
        .map(|id| id as u64)
        .collect::<Vec<_>>();

    Ok(Json(
        get_tracks(&data.database, Some(&ids))
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
        get_album_tracks(query.album_id, &data)
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
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiAlbumVersion>>> {
    Ok(Json(
        get_album_versions(query.album_id, &data)
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
        get_artist_albums(query.artist_id, &data)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch albums"))?
            .iter()
            .map(|t| t.to_api())
            .collect(),
    ))
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
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to fetch artist: {e:?}")))?
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
    Ok(Json(
        get_album(
            &data.database,
            query.album_id,
            query.tidal_album_id,
            query.qobuz_album_id.clone(),
        )
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
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        add_album(
            data.database.clone(),
            &album_id_for_source(&query.album_id, query.source)?.into(),
            &*music_api_from_source(data.database.clone(), query.source),
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
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        remove_album(
            data.database.clone(),
            &album_id_for_source(&query.album_id, query.source)?.into(),
            &*music_api_from_source(data.database.clone(), query.source),
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
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        refavorite_album(
            data.database.clone(),
            &album_id_for_source(&query.album_id, query.source)?.into(),
            &*music_api_from_source(data.database.clone(), query.source),
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to re-favorite album: {e:?}")))?
        .to_api(),
    ))
}
