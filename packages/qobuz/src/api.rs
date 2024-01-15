use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use moosicbox_core::sqlite::models::ToApi;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    album_tracks, favorite_albums, QobuzAlbum, QobuzAlbumTracksError, QobuzArtist,
    QobuzFavoriteAlbumsError, QobuzTrack,
};

impl ToApi<ApiQobuzAlbum> for QobuzAlbum {
    fn to_api(&self) -> ApiQobuzAlbum {
        ApiQobuzAlbum {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            cover: self.cover_url(1280),
            duration: self.duration,
            popularity: self.popularity,
            title: self.title.clone(),
            parental_warning: self.parental_warning,
            track_count: self.tracks_count,
            release_date: self.released_at.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub cover: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub track_count: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
}

impl ToApi<ApiQobuzTrack> for QobuzTrack {
    fn to_api(&self) -> ApiQobuzTrack {
        ApiQobuzTrack {
            id: self.id,
            track_number: self.track_number,
            album_id: self.album_id,
            artist_id: self.artist_id,
            duration: self.duration,
            parental_warning: self.parental_warning,
            isrc: self.isrc.clone(),
            popularity: self.popularity,
            title: self.title.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub album_id: u64,
    pub artist_id: u64,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
}

impl ToApi<ApiQobuzArtist> for QobuzArtist {
    fn to_api(&self) -> ApiQobuzArtist {
        ApiQobuzArtist {
            id: self.id,
            picture: self.picture_url(750),
            popularity: self.popularity,
            name: self.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub popularity: u32,
    pub name: String,
}

static QOBUZ_ACCESS_TOKEN_HEADER: &str = "x-qobuz-access-token";
static QOBUZ_APP_ID_HEADER: &str = "x-qobuz-app-id";

impl From<QobuzFavoriteAlbumsError> for actix_web::Error {
    fn from(err: QobuzFavoriteAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/qobuz/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_albums(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        query.offset,
        query.limit,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<QobuzAlbumTracksError> for actix_web::Error {
    fn from(err: QobuzAlbumTracksError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbumTracksQuery {
    album_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/qobuz/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = album_tracks(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        &query.album_id,
        query.offset,
        query.limit,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}
