use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get};
use moosicbox_core::{
    app::AppState,
    slim::{
        menu::{get_all_artists, ApiAlbum, ApiArtist, ArtistFilters, ArtistSort, ArtistsRequest},
        player::ApiTrack,
    },
    sqlite::menu::get_album,
    ToApi,
};
use moosicbox_core::{
    slim::menu::{get_all_albums, Album, AlbumFilters, AlbumSort, AlbumSource, AlbumsRequest},
    sqlite::menu::get_album_tracks,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use thiserror::Error;

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
    Albums(Vec<Album>),
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
    player_id: String,
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    artist: Option<String>,
    search: Option<String>,
}

#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiAlbum>>> {
    let player_id = &query.player_id;
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
        },
    };

    Ok(Json(
        get_all_albums(player_id, &data, &request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e:?}")))?
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
pub struct GetAlbumQuery {
    album_id: i32,
}

#[get("/album")]
pub async fn get_album_endpoint(
    query: web::Query<GetAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        get_album(query.album_id, &data)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch album"))?
            .to_api(),
    ))
}
