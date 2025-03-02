#![allow(clippy::module_name_repetitions)]

use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized},
    route,
    web::{self, Json},
    HttpRequest, Result, Scope,
};
#[cfg(feature = "db")]
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_music_models::{
    api::{ApiAlbum, ApiArtist},
    ApiSource, ApiSources, TrackApiSource,
};
use moosicbox_paging::Page;
use moosicbox_search::api::models::ApiSearchResultsResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    add_favorite_album, add_favorite_artist, add_favorite_track, album, album_tracks, artist,
    artist_albums, device_authorization, device_authorization_token, favorite_albums,
    favorite_artists, favorite_tracks, models::TidalAlbum, remove_favorite_album,
    remove_favorite_artist, remove_favorite_track, search, track, track_file_url,
    track_playback_info, AuthenticatedRequestError, SearchType, TidalAddFavoriteAlbumError,
    TidalAddFavoriteArtistError, TidalAddFavoriteTrackError, TidalAlbumError, TidalAlbumOrder,
    TidalAlbumOrderDirection, TidalAlbumTracksError, TidalAlbumType, TidalArtistAlbumsError,
    TidalArtistError, TidalArtistOrder, TidalArtistOrderDirection, TidalAudioQuality,
    TidalDeviceAuthorizationError, TidalDeviceAuthorizationTokenError, TidalDeviceType,
    TidalFavoriteAlbumsError, TidalFavoriteArtistsError, TidalFavoriteTracksError,
    TidalRemoveFavoriteAlbumError, TidalRemoveFavoriteArtistError, TidalRemoveFavoriteTrackError,
    TidalSearchError, TidalTrack, TidalTrackError, TidalTrackFileUrlError, TidalTrackOrder,
    TidalTrackOrderDirection, TidalTrackPlaybackInfo, TidalTrackPlaybackInfoError,
};

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(device_authorization_endpoint)
        .service(device_authorization_token_endpoint)
        .service(track_file_url_endpoint)
        .service(track_playback_info_endpoint)
        .service(favorite_artists_endpoint)
        .service(add_favorite_artist_endpoint)
        .service(remove_favorite_artist_endpoint)
        .service(favorite_albums_endpoint)
        .service(add_favorite_album_endpoint)
        .service(remove_favorite_album_endpoint)
        .service(favorite_tracks_endpoint)
        .service(add_favorite_track_endpoint)
        .service(remove_favorite_track_endpoint)
        .service(artist_albums_endpoint)
        .service(album_tracks_endpoint)
        .service(album_endpoint)
        .service(artist_endpoint)
        .service(track_endpoint)
        .service(search_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Tidal")),
    paths(
        device_authorization_endpoint,
        device_authorization_token_endpoint,
        track_file_url_endpoint,
        track_playback_info_endpoint,
        favorite_artists_endpoint,
        add_favorite_artist_endpoint,
        remove_favorite_artist_endpoint,
        favorite_albums_endpoint,
        add_favorite_album_endpoint,
        remove_favorite_album_endpoint,
        add_favorite_track_endpoint,
        remove_favorite_track_endpoint,
        favorite_tracks_endpoint,
        artist_albums_endpoint,
        album_tracks_endpoint,
        album_endpoint,
        artist_endpoint,
        track_endpoint,
        search_endpoint,
    ),
    components(schemas(
        TidalTrackPlaybackInfo,
        TidalDeviceType,
        TidalAudioQuality,
        TidalArtistOrder,
        TidalArtistOrderDirection,
        TidalAlbumOrder,
        TidalAlbumOrderDirection,
        TidalTrackOrder,
        TidalTrackOrderDirection,
    ))
)]
pub struct Api;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    pub id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: TidalAlbumType,
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
    pub api_source: ApiSource,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    Tidal(ApiTidalTrack),
}

impl From<TidalTrack> for ApiTrack {
    fn from(value: TidalTrack) -> Self {
        Self::Tidal(ApiTidalTrack {
            contains_cover: value.album_cover.is_some(),
            id: value.id,
            number: value.track_number,
            album: value.album,
            album_id: value.album_id,
            album_type: value.album_type,
            artist: value.artist,
            artist_id: value.artist_id,
            audio_quality: value.audio_quality,
            copyright: value.copyright,
            duration: value.duration,
            explicit: value.explicit,
            isrc: value.isrc,
            popularity: value.popularity,
            title: value.title,
            media_metadata_tags: value.media_metadata_tags,
            api_source: ApiSource::Tidal,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalTrack {
    pub id: u64,
    pub number: u32,
    pub album: String,
    pub album_id: u64,
    pub album_type: TidalAlbumType,
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
    pub api_source: ApiSource,
}

impl From<ApiTidalTrack> for moosicbox_music_models::api::ApiTrack {
    fn from(value: ApiTidalTrack) -> Self {
        Self {
            track_id: value.id.into(),
            number: value.number,
            title: value.title,
            duration: f64::from(value.duration),
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: None,
            date_added: None,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            contains_cover: value.contains_cover,
            blur: false,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: TrackApiSource::Tidal,
            api_source: ApiSource::Tidal,
            sources: ApiSources::default().with_source(ApiSource::Tidal, value.id.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalArtist {
    pub id: u64,
    pub contains_cover: bool,
    pub popularity: u32,
    pub title: String,
    pub api_source: ApiSource,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        post,
        path = "/auth/device-authorization",
        description = "Begin the authorization process for Tidal",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("clientId" = String, Query, description = "Tidal client ID to use"),
            ("open" = Option<bool>, Query, description = "Open the authorization page in a browser"),
        ),
        responses(
            (
                status = 200,
                description = "URL and Device code used in the Tidal authorization flow",
                body = Value,
            )
        )
    )
)]
#[route("/auth/device-authorization", method = "POST")]
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        post,
        path = "/auth/device-authorization/token",
        description = "Finish the authorization process for Tidal",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("clientId" = String, Query, description = "Tidal client ID to use"),
            ("clientSecret" = String, Query, description = "Tidal client secret to use"),
            ("deviceCode" = String, Query, description = "Tidal device code to use"),
        ),
        responses(
            (
                status = 200,
                description = "Access token and refresh token used in the Tidal authentication",
                body = Value,
            )
        )
    )
)]
#[route("/auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    query: web::Query<TidalDeviceAuthorizationTokenQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization_token(
            #[cfg(feature = "db")]
            &db,
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
    fn from(e: TidalTrackFileUrlError) -> Self {
        match e {
            TidalTrackFileUrlError::AuthenticatedRequest(e) => ErrorUnauthorized(e.to_string()),
            TidalTrackFileUrlError::Parse(_) => ErrorInternalServerError(e.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackFileUrlQuery {
    audio_quality: TidalAudioQuality,
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/track/url",
        description = "Get Tidal track file stream URL",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("audioQuality" = TidalAudioQuality, Query, description = "Audio quality to fetch the file stream for"),
            ("trackId" = u64, Query, description = "Tidal track ID to fetch track stream URL for"),
        ),
        responses(
            (
                status = 200,
                description = "Tidal track URL for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/track/url", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackFileUrlQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            #[cfg(feature = "db")]
            &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/track/playback-info",
        description = "Get Tidal track metadata info",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("audioQuality" = TidalAudioQuality, Query, description = "Audio quality to fetch the track metadata for"),
            ("trackId" = u64, Query, description = "Tidal track ID to fetch track metadata for"),
        ),
        responses(
            (
                status = 200,
                description = "Tidal track metadata info",
                body = TidalTrackPlaybackInfo,
            )
        )
    )
)]
#[route("/track/playback-info", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn track_playback_info_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackPlaybackInfoQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<TidalTrackPlaybackInfo>> {
    Ok(Json(
        track_playback_info(
            #[cfg(feature = "db")]
            &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/favorites/artists",
        description = "Get Tidal favorited artists",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<TidalArtistOrder>, Query, description = "Sort property to sort the artists by"),
            ("orderDirection" = Option<TidalAlbumOrderDirection>, Query, description = "Sort order direction to order the artists by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Tidal favorited artists",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteArtistsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
            #[cfg(feature = "db")]
            &db,
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
        .map(Into::into)
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        post,
        path = "/favorites/artists",
        description = "Favorite a Tidal artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Tidal artist ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "POST")]
#[allow(clippy::future_not_send)]
pub async fn add_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteArtistsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_artist(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        delete,
        path = "/favorites/artists",
        description = "Remove Tidal artist from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Tidal artist ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "DELETE")]
#[allow(clippy::future_not_send)]
pub async fn remove_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteArtistsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_artist(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/favorites/albums",
        description = "Get Tidal favorited albums",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<TidalAlbumOrder>, Query, description = "Sort property to sort the albums by"),
            ("orderDirection" = Option<TidalAlbumOrderDirection>, Query, description = "Sort order direction to order the albums by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Tidal favorited albums",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        favorite_albums(
            #[cfg(feature = "db")]
            &db,
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
        .map(|x| {
            x.try_into()
                .map_err(|e| TidalFavoriteAlbumsError::RequestFailed(format!("{e:?}")))
                as Result<ApiAlbum, TidalFavoriteAlbumsError>
        })
        .transpose()
        .map_err(ErrorInternalServerError)?
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        post,
        path = "/favorites/albums",
        description = "Favorite a Tidal album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "Tidal album ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "POST")]
#[allow(clippy::future_not_send)]
pub async fn add_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_album(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        delete,
        path = "/favorites/albums",
        description = "Remove Tidal album from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "Tidal album ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "DELETE")]
#[allow(clippy::future_not_send)]
pub async fn remove_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_album(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        post,
        path = "/favorites/tracks",
        description = "Favorite a Tidal track",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "Tidal track ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "POST")]
#[allow(clippy::future_not_send)]
pub async fn add_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAddFavoriteTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_track(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        delete,
        path = "/favorites/tracks",
        description = "Remove Tidal track from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "Tidal track ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "DELETE")]
#[allow(clippy::future_not_send)]
pub async fn remove_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalRemoveFavoriteTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_track(
        #[cfg(feature = "db")]
        &db,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/favorites/tracks",
        description = "Get Tidal favorited tracks",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<TidalTrackOrder>, Query, description = "Sort property to sort the tracks by"),
            ("orderDirection" = Option<TidalTrackOrderDirection>, Query, description = "Sort order direction to order the tracks by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Tidal favorited tracks",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<TidalTrack> = favorite_tracks(
        #[cfg(feature = "db")]
        &db,
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
    .into();

    Ok(Json(tracks.into()))
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<AlbumType> for TidalAlbumType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => Self::Lp,
            AlbumType::EpsAndSingles => Self::EpsAndSingles,
            AlbumType::Compilations => Self::Compilations,
        }
    }
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/artists/albums",
        description = "Get Tidal albums for the specified artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Tidal artist ID to search for albums for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("albumType" = Option<AlbumType>, Query, description = "Album type to filter to"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Tidal albums for an artist",
                body = Value,
            )
        )
    )
)]
#[route("/artists/albums", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    let albums: Page<TidalAlbum> = artist_albums(
        #[cfg(feature = "db")]
        &db,
        &query.artist_id.into(),
        query.offset,
        query.limit,
        query.album_type.map(Into::into),
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    Ok(Json(albums.try_into().map_err(ErrorInternalServerError)?))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/albums/tracks",
        description = "Get Tidal tracks for the specified album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "Tidal album ID to search for tracks for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Tidal tracks for an album",
                body = Value,
            )
        )
    )
)]
#[route("/albums/tracks", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAlbumTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<TidalTrack> = album_tracks(
        #[cfg(feature = "db")]
        &db,
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
    .into();

    Ok(Json(tracks.into()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/albums",
        description = "Get Tidal album for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "Tidal album ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Tidal album for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/albums", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<TidalAlbumQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        &db,
        &query.album_id.into(),
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.try_into().map_err(ErrorInternalServerError)?))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/artists",
        description = "Get Tidal artist for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Tidal artist ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Tidal artist for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/artists", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<TidalArtistQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        &db,
        &query.artist_id.into(),
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.into()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/tracks",
        description = "Get Tidal track for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "Tidal track ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Tidal track for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/tracks", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        &db,
        &query.track_id.into(),
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.into()))
}

impl From<TidalSearchError> for actix_web::Error {
    fn from(err: TidalSearchError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
    include_contributions: Option<bool>,
    include_did_you_mean: Option<bool>,
    include_user_playlists: Option<bool>,
    supports_user_data: Option<bool>,
    types: Option<Vec<SearchType>>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Tidal"],
        get,
        path = "/search",
        description = "Search the Tidal library for artists/albums/tracks that fuzzy match the query",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("query" = String, Query, description = "The search query"),
            ("offset" = Option<usize>, Query, description = "Page offset"),
            ("limit" = Option<usize>, Query, description = "Page limit"),
            ("include_contributions" = Option<bool>, Query, description = "Include contribution results"),
            ("include_did_you_mean" = Option<bool>, Query, description = "Include 'did you mean?' results"),
            ("include_user_playlists" = Option<bool>, Query, description = "Include user playlists"),
            ("supports_user_data" = Option<bool>, Query, description = "Include user data"),
            ("types" = Option<Vec<SearchType>>, Query, description = "Search types to search across"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<TidalDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "A page of matches for the given search query",
                body = ApiSearchResultsResponse,
            )
        )
    )
)]
#[route("/search", method = "GET")]
#[allow(clippy::future_not_send)]
pub async fn search_endpoint(
    req: HttpRequest,
    query: web::Query<TidalSearchQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiSearchResultsResponse>> {
    let results = search(
        #[cfg(feature = "db")]
        &db,
        &query.query,
        query.offset,
        query.limit,
        query.include_contributions,
        query.include_did_you_mean,
        query.include_user_playlists,
        query.supports_user_data,
        query
            .types
            .clone()
            .map(|x| x.into_iter().map(Into::into).collect::<Vec<_>>()),
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(results.into()))
}
