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
    album, album_tracks, artist, artist_albums, favorite_albums, favorite_artists, favorite_tracks,
    track, track_file_url, user_login, QobuzAlbum, QobuzAlbumError, QobuzAlbumTracksError,
    QobuzArtist, QobuzArtistAlbumsError, QobuzArtistError, QobuzAudioQuality,
    QobuzFavoriteAlbumsError, QobuzFavoriteArtistsError, QobuzFavoriteTracksError, QobuzTrack,
    QobuzTrackError, QobuzTrackFileUrlError, QobuzUserLoginError,
};

impl ToApi<ApiQobuzAlbum> for QobuzAlbum {
    fn to_api(&self) -> ApiQobuzAlbum {
        ApiQobuzAlbum {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            cover: self.cover_url(),
            duration: self.duration,
            title: self.title.clone(),
            parental_warning: self.parental_warning,
            track_count: self.tracks_count,
            release_date: self.release_date_original.clone(),
        }
    }
}

impl From<QobuzUserLoginError> for actix_web::Error {
    fn from(err: QobuzUserLoginError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzUserLoginQuery {
    username: String,
    password: String,
    #[cfg(feature = "db")]
    persist: Option<bool>,
}

#[route("/qobuz/auth/login", method = "POST")]
pub async fn user_login_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzUserLoginQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        user_login(
            #[cfg(feature = "db")]
            data.db.as_ref().unwrap(),
            &query.username,
            &query.password,
            req.headers()
                .get(QOBUZ_APP_ID_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            #[cfg(feature = "db")]
            query.persist,
        )
        .await?,
    ))
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub cover: Option<String>,
    pub duration: u32,
    pub parental_warning: bool,
    pub track_count: u32,
    pub release_date: String,
    pub title: String,
}

impl ToApi<ApiQobuzTrack> for QobuzTrack {
    fn to_api(&self) -> ApiQobuzTrack {
        ApiQobuzTrack {
            id: self.id,
            track_number: self.track_number,
            album_id: self.album_id.clone(),
            artist_id: self.artist_id,
            duration: self.duration,
            parental_warning: self.parental_warning,
            isrc: self.isrc.clone(),
            title: self.title.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub album_id: String,
    pub artist_id: u64,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub title: String,
}

impl ToApi<ApiQobuzArtist> for QobuzArtist {
    fn to_api(&self) -> ApiQobuzArtist {
        ApiQobuzArtist {
            id: self.id,
            cover: self.cover_url(),
            name: self.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzArtist {
    pub id: u64,
    pub cover: Option<String>,
    pub name: String,
}

static QOBUZ_ACCESS_TOKEN_HEADER: &str = "x-qobuz-access-token";
static QOBUZ_APP_ID_HEADER: &str = "x-qobuz-app-id";
static QOBUZ_APP_SECRET_HEADER: &str = "x-qobuz-app-secret";

impl From<QobuzArtistError> for actix_web::Error {
    fn from(err: QobuzArtistError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtistQuery {
    artist_id: u64,
}

#[route("/qobuz/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<QobuzArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        query.artist_id,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist))
}

impl From<QobuzFavoriteArtistsError> for actix_web::Error {
    fn from(err: QobuzFavoriteArtistsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteArtistsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/qobuz/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_artists(
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

impl From<QobuzAlbumError> for actix_web::Error {
    fn from(err: QobuzAlbumError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbumQuery {
    album_id: String,
}

#[route("/qobuz/albums", method = "GET")]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<QobuzAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        &query.album_id,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album))
}

impl From<QobuzArtistAlbumsError> for actix_web::Error {
    fn from(err: QobuzArtistAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtistAlbumsQuery {
    artist_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/qobuz/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = artist_albums(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        &query.artist_id,
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

impl From<QobuzTrackError> for actix_web::Error {
    fn from(err: QobuzTrackError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrackQuery {
    track_id: u64,
}

#[route("/qobuz/tracks", method = "GET")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<QobuzTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        query.track_id,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track))
}

impl From<QobuzFavoriteTracksError> for actix_web::Error {
    fn from(err: QobuzFavoriteTracksError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteTracksQuery {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/qobuz/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_tracks(
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

impl From<QobuzTrackFileUrlError> for actix_web::Error {
    fn from(err: QobuzTrackFileUrlError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrackFileUrlQuery {
    audio_quality: QobuzAudioQuality,
    track_id: u64,
}

#[route("/tidal/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackFileUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<String>> {
    Ok(Json(
        track_file_url(
            #[cfg(feature = "db")]
            data.db.as_ref().unwrap(),
            query.track_id,
            query.audio_quality,
            req.headers()
                .get(QOBUZ_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            req.headers()
                .get(QOBUZ_APP_ID_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            req.headers()
                .get(QOBUZ_APP_SECRET_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}
