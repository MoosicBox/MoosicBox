use actix_web::{
    error::{ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use moosicbox_core::sqlite::models::ToApi;
use moosicbox_music_api::Page;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    add_favorite_album, add_favorite_artist, add_favorite_track, album, album_tracks, artist,
    artist_albums, device_authorization, device_authorization_token, favorite_albums,
    favorite_artists, favorite_tracks, remove_favorite_album, remove_favorite_artist,
    remove_favorite_track, track, track_file_url, track_playback_info, AuthenticatedRequestError,
    TidalAddFavoriteAlbumError, TidalAddFavoriteArtistError, TidalAddFavoriteTrackError,
    TidalAlbum, TidalAlbumError, TidalAlbumOrder, TidalAlbumOrderDirection, TidalAlbumTracksError,
    TidalAlbumType, TidalArtist, TidalArtistAlbumsError, TidalArtistError, TidalArtistOrder,
    TidalArtistOrderDirection, TidalAudioQuality, TidalDeviceAuthorizationError,
    TidalDeviceAuthorizationTokenError, TidalDeviceType, TidalFavoriteAlbumsError,
    TidalFavoriteArtistsError, TidalFavoriteTracksError, TidalRemoveFavoriteAlbumError,
    TidalRemoveFavoriteArtistError, TidalRemoveFavoriteTrackError, TidalTrack, TidalTrackError,
    TidalTrackFileUrlError, TidalTrackOrder, TidalTrackOrderDirection, TidalTrackPlaybackInfo,
    TidalTrackPlaybackInfoError,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Tidal(ApiTidalAlbum),
}

impl ToApi<ApiAlbum> for TidalAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Tidal(ApiTidalAlbum {
            id: self.id,
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.contains_cover,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            duration: self.duration,
            explicit: self.explicit,
            number_of_tracks: self.number_of_tracks,
            popularity: self.popularity,
            date_released: self.release_date.clone(),
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    pub id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub date_released: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    Tidal(ApiTidalTrack),
}

impl ToApi<ApiTrack> for TidalTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Tidal(ApiTidalTrack {
            id: self.id,
            number: self.track_number,
            album: self.album.clone(),
            album_id: self.album_id,
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.album_cover.is_some(),
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            duration: self.duration,
            explicit: self.explicit,
            isrc: self.isrc.clone(),
            popularity: self.popularity,
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalTrack {
    pub id: u64,
    pub number: u32,
    pub album: String,
    pub album_id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Tidal(ApiTidalArtist),
}

impl ToApi<ApiArtist> for TidalArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Tidal(ApiTidalArtist {
            id: self.id,
            contains_cover: self.contains_cover,
            popularity: self.popularity,
            title: self.name.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalArtist {
    pub id: u64,
    pub contains_cover: bool,
    pub popularity: u32,
    pub title: String,
}

static TIDAL_ACCESS_TOKEN_HEADER: &str = "x-tidal-access-token";

impl From<TidalDeviceAuthorizationError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationError) -> Self {
        log::error!("{err:?}");
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
        log::error!("{err:?}");
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
            data.db.as_ref().unwrap(),
            query.client_id.clone(),
            query.client_secret.clone(),
            query.device_code.clone(),
            #[cfg(feature = "db")]
            query.persist,
        )
        .await?,
    ))
}

impl From<TidalTrackFileUrlError> for actix_web::Error {
    fn from(err: TidalTrackFileUrlError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackFileUrlQuery {
    audio_quality: TidalAudioQuality,
    track_id: u64,
}

#[route("/tidal/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackFileUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            #[cfg(feature = "db")]
            data.db.as_ref().unwrap(),
            query.audio_quality,
            &query.track_id.into(),
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    })))
}

impl From<TidalTrackPlaybackInfoError> for actix_web::Error {
    fn from(err: TidalTrackPlaybackInfoError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackPlaybackInfoQuery {
    audio_quality: TidalAudioQuality,
    track_id: u64,
}

#[route("/tidal/track/playback-info", method = "GET")]
pub async fn track_playback_info_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackPlaybackInfoQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<TidalTrackPlaybackInfo>> {
    Ok(Json(
        track_playback_info(
            #[cfg(feature = "db")]
            data.db.as_ref().unwrap(),
            query.audio_quality,
            &query.track_id.into(),
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}

impl From<TidalFavoriteArtistsError> for actix_web::Error {
    fn from(err: TidalFavoriteArtistsError) -> Self {
        log::error!("{err:?}");
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
    user_id: Option<u64>,
}

#[route("/tidal/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
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
        .await?
        .to_api()
        .into(),
    ))
}

impl From<TidalAddFavoriteArtistError> for actix_web::Error {
    fn from(err: TidalAddFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAddFavoriteArtistsQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/artists", method = "POST")]
pub async fn add_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_artist(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.artist_id.into(),
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
        "success": true
    })))
}

impl From<TidalRemoveFavoriteArtistError> for actix_web::Error {
    fn from(err: TidalRemoveFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalRemoveFavoriteArtistsQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/artists", method = "DELETE")]
pub async fn remove_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_artist(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.artist_id.into(),
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
        "success": true
    })))
}

impl From<TidalFavoriteAlbumsError> for actix_web::Error {
    fn from(err: TidalFavoriteAlbumsError) -> Self {
        log::error!("{err:?}");
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
    user_id: Option<u64>,
}

#[route("/tidal/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        favorite_albums(
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
        .await?
        .to_api()
        .into(),
    ))
}

impl From<TidalAddFavoriteAlbumError> for actix_web::Error {
    fn from(err: TidalAddFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAddFavoriteAlbumsQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/albums", method = "POST")]
pub async fn add_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_album(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.album_id.into(),
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
        "success": true
    })))
}

impl From<TidalRemoveFavoriteAlbumError> for actix_web::Error {
    fn from(err: TidalRemoveFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalRemoveFavoriteAlbumsQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/albums", method = "DELETE")]
pub async fn remove_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_album(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.album_id.into(),
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
        "success": true
    })))
}

impl From<TidalAddFavoriteTrackError> for actix_web::Error {
    fn from(err: TidalAddFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAddFavoriteTracksQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/tracks", method = "POST")]
pub async fn add_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_track(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.track_id.into(),
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
        "success": true
    })))
}

impl From<TidalRemoveFavoriteTrackError> for actix_web::Error {
    fn from(err: TidalRemoveFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalRemoveFavoriteTracksQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    user_id: Option<u64>,
}

#[route("/tidal/favorites/tracks", method = "DELETE")]
pub async fn remove_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_track(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.track_id.into(),
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
        "success": true
    })))
}

impl From<TidalFavoriteTracksError> for actix_web::Error {
    fn from(err: TidalFavoriteTracksError) -> Self {
        log::error!("{err:?}");
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
    user_id: Option<u64>,
}

#[route("/tidal/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        favorite_tracks(
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
        .await?
        .to_api()
        .into(),
    ))
}

impl From<TidalArtistAlbumsError> for actix_web::Error {
    fn from(err: TidalArtistAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistAlbumsQuery {
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<AlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumType {
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<AlbumType> for TidalAlbumType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => TidalAlbumType::Lp,
            AlbumType::EpsAndSingles => TidalAlbumType::EpsAndSingles,
            AlbumType::Compilations => TidalAlbumType::Compilations,
        }
    }
}

#[route("/tidal/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        artist_albums(
            #[cfg(feature = "db")]
            data.db.as_ref().unwrap(),
            &query.artist_id.into(),
            query.offset,
            query.limit,
            query.album_type.map(|t| t.into()),
            query.country_code.clone(),
            query.locale.clone(),
            query.device_type,
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<TidalAlbumTracksError> for actix_web::Error {
    fn from(err: TidalAlbumTracksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbumTracksQuery {
    album_id: u64,
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
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        album_tracks(
            #[cfg(feature = "db")]
            data.db.as_ref().expect("Db not set"),
            &query.album_id.into(),
            query.offset,
            query.limit,
            query.country_code.clone(),
            query.locale.clone(),
            query.device_type,
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<TidalAlbumError> for actix_web::Error {
    fn from(err: TidalAlbumError) -> Self {
        log::error!("{err:?}");
        if let TidalAlbumError::AuthenticatedRequest(AuthenticatedRequestError::RequestFailed(
            status,
            _,
        )) = err
        {
            if status == 404 {
                return ErrorNotFound("Tidal album not found");
            }
        }

        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbumQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/albums", method = "GET")]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAlbumQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.album_id.into(),
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
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        &query.artist_id.into(),
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
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/tracks", method = "GET")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        data.db.as_ref().unwrap(),
        &query.track_id.into(),
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
