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
    album, album_tracks, artist, artist_albums, device_authorization, device_authorization_token,
    favorite_albums, favorite_artists, favorite_tracks, track, track_url, TidalAlbum,
    TidalAlbumError, TidalAlbumOrder, TidalAlbumOrderDirection, TidalAlbumTracksError, TidalArtist,
    TidalArtistAlbumsError, TidalArtistError, TidalArtistOrder, TidalArtistOrderDirection,
    TidalAudioQuality, TidalDeviceAuthorizationError, TidalDeviceAuthorizationTokenError,
    TidalDeviceType, TidalFavoriteAlbumsError, TidalFavoriteArtistsError, TidalFavoriteTracksError,
    TidalTrack, TidalTrackError, TidalTrackOrder, TidalTrackOrderDirection, TidalTrackUrlError,
};

impl ToApi<ApiTidalAlbum> for TidalAlbum {
    fn to_api(&self) -> ApiTidalAlbum {
        ApiTidalAlbum {
            id: self.id,
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            cover: self.cover_url(1280),
            duration: self.duration,
            explicit: self.explicit,
            number_of_tracks: self.number_of_tracks,
            popularity: self.popularity,
            release_date: self.release_date.clone(),
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    pub id: u32,
    pub artist: String,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl ToApi<ApiTidalTrack> for TidalTrack {
    fn to_api(&self) -> ApiTidalTrack {
        ApiTidalTrack {
            id: self.id,
            track_number: self.track_number,
            album_id: self.album_id,
            artist_id: self.artist_id,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            duration: self.duration,
            explicit: self.explicit,
            isrc: self.isrc.clone(),
            popularity: self.popularity,
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalTrack {
    pub id: u32,
    pub track_number: u32,
    pub album_id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl ToApi<ApiTidalArtist> for TidalArtist {
    fn to_api(&self) -> ApiTidalArtist {
        ApiTidalArtist {
            id: self.id,
            picture: self.picture_url(750),
            popularity: self.popularity,
            name: self.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalArtist {
    pub id: u32,
    pub picture: Option<String>,
    pub popularity: u32,
    pub name: String,
}

static TIDAL_ACCESS_TOKEN_HEADER: &str = "x-tidal-access-token";

impl From<TidalDeviceAuthorizationError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationQuery {
    client_id: String,
    open: Option<bool>,
}

#[route("/tidal/auth/device-authorization", method = "POST")]
pub async fn device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization(query.client_id.clone(), query.open.unwrap_or(false)).await?,
    ))
}

impl From<TidalDeviceAuthorizationTokenError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationTokenError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationTokenQuery {
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")]
    persist: Option<bool>,
}

#[route("/tidal/auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    query: web::Query<TidalDeviceAuthorizationTokenQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization_token(
            #[cfg(feature = "db")]
            data.db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap(),
            query.client_id.clone(),
            query.client_secret.clone(),
            query.device_code.clone(),
            #[cfg(feature = "db")]
            query.persist,
        )
        .await?,
    ))
}

impl From<TidalTrackUrlError> for actix_web::Error {
    fn from(err: TidalTrackUrlError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackUrlQuery {
    audio_quality: TidalAudioQuality,
    track_id: u32,
}

#[route("/tidal/track/url", method = "GET")]
pub async fn track_url_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        track_url(
            #[cfg(feature = "db")]
            data.db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap(),
            query.audio_quality,
            query.track_id,
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}

impl From<TidalFavoriteArtistsError> for actix_web::Error {
    fn from(err: TidalFavoriteArtistsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteArtistsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalArtistOrder>,
    order_direction: Option<TidalArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u32>,
}

#[route("/tidal/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_artists(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.offset,
        query.limit,
        query.order,
        query.order_direction,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<TidalFavoriteAlbumsError> for actix_web::Error {
    fn from(err: TidalFavoriteAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u32>,
}

#[route("/tidal/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_albums(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        query.offset,
        query.limit,
        query.order,
        query.order_direction,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<TidalFavoriteTracksError> for actix_web::Error {
    fn from(err: TidalFavoriteTracksError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteTracksQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalTrackOrder>,
    order_direction: Option<TidalTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u32>,
}

#[route("/tidal/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = favorite_tracks(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.offset,
        query.limit,
        query.order,
        query.order_direction,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<TidalArtistAlbumsError> for actix_web::Error {
    fn from(err: TidalArtistAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistAlbumsQuery {
    artist_id: u32,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = artist_albums(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.artist_id,
        query.offset,
        query.limit,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<TidalAlbumTracksError> for actix_web::Error {
    fn from(err: TidalAlbumTracksError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbumTracksQuery {
    album_id: u32,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAlbumTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, count) = album_tracks(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.album_id,
        query.offset,
        query.limit,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "count": count,
        "items": items.iter().map(|item| item.to_api()).collect::<Vec<_>>(),
    })))
}

impl From<TidalAlbumError> for actix_web::Error {
    fn from(err: TidalAlbumError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbumQuery {
    album_id: u32,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/albums", method = "GET")]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAlbumQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTidalAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.album_id,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.to_api()))
}

impl From<TidalArtistError> for actix_web::Error {
    fn from(err: TidalArtistError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistQuery {
    artist_id: u32,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTidalArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.artist_id,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.to_api()))
}

impl From<TidalTrackError> for actix_web::Error {
    fn from(err: TidalTrackError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackQuery {
    track_id: u32,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/tracks", method = "GET")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTidalTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        data.db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap(),
        query.track_id,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.to_api()))
}
