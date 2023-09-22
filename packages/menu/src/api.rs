use actix_web::{
    error::ErrorBadRequest,
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get};
use moosicbox_core::{app::AppState, slim::player::Track, sqlite::menu::get_album};
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
pub struct GetAlbumsQuery {
    player_id: String,
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    artist: Option<String>,
    year: Option<String>,
    search: Option<String>,
}

#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<Album>>> {
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
            year: query
                .year
                .clone()
                .map(|y| {
                    y.parse::<i16>()
                        .map_err(|_e| ErrorBadRequest(format!("Invalid year filter value: {y}")))
                })
                .transpose()?,
            search: query.search.clone().map(|s| s.to_lowercase()),
        },
    };

    match get_all_albums(player_id, &data, &request).await {
        Ok(resp) => Ok(Json(resp)),
        Err(error) => panic!("Failed to get albums: {:?}", error),
    }
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
) -> Result<Json<Vec<Track>>> {
    match get_album_tracks(query.album_id, &data).await {
        Ok(resp) => Ok(Json(resp)),
        Err(error) => panic!("Failed to get album tracks: {:?}", error),
    }
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
) -> Result<Json<Album>> {
    match get_album(query.album_id, &data).await {
        Ok(resp) => Ok(Json(resp)),
        Err(error) => panic!("Failed to get album tracks: {:?}", error),
    }
}
