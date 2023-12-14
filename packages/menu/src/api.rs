use std::str::FromStr;

use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get};
use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::get_tracks,
        menu::{get_album, get_artist},
        models::{Album, AlbumSort, AlbumSource, ApiAlbum, ApiArtist, ApiTrack, ArtistSort, ToApi},
    },
    track_range::{parse_track_id_ranges, ParseTrackIdsError},
};
use moosicbox_core::{sqlite::menu::get_album_tracks, sqlite::menu::get_artist_albums};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::library::{
    albums::{get_all_albums, AlbumFilters, AlbumsRequest},
    artists::{get_all_artists, ArtistFilters, ArtistsRequest},
};

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
    let ids = parse_track_id_ranges(&query.track_ids).map_err(|e| match e {
        ParseTrackIdsError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseTrackIdsError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseTrackIdsError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;
    Ok(Json(
        get_tracks(
            &data
                .db
                .as_ref()
                .expect("DB not set")
                .library
                .lock()
                .unwrap()
                .inner,
            &ids,
        )
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
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistQuery {
    artist_id: i32,
}

#[get("/artist")]
pub async fn get_artist_endpoint(
    query: web::Query<GetArtistQuery>,
    data: web::Data<AppState>,
) -> Result<Json<ApiArtist>> {
    Ok(Json(
        get_artist(query.artist_id, &data)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch artist"))?
            .to_api(),
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
