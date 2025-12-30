//! HTTP API endpoints for Tidal integration.
//!
//! This module provides Actix Web route handlers for the Tidal music streaming service,
//! including device authorization, favorites management (artists, albums, tracks),
//! track playback information retrieval, and search functionality.

#![allow(clippy::needless_for_each)]
#![allow(clippy::module_name_repetitions)]

use actix_web::{
    HttpRequest, Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized},
    route,
    web::{self, Json},
};
use moosicbox_music_api::models::search::api::ApiSearchResultsResponse;
use moosicbox_music_models::{
    ApiSource, ApiSources,
    api::{ApiAlbum, ApiArtist},
};
use moosicbox_paging::Page;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
#[cfg(feature = "db")]
use switchy::database::profiles::LibraryDatabase;

use crate::{
    API_SOURCE, Error, SearchType, TidalAlbumOrder, TidalAlbumOrderDirection, TidalAlbumType,
    TidalArtistOrder, TidalArtistOrderDirection, TidalAudioQuality, TidalDeviceType, TidalTrack,
    TidalTrackOrder, TidalTrackOrderDirection, TidalTrackPlaybackInfo, add_favorite_album,
    add_favorite_artist, add_favorite_track, album, album_tracks, artist, artist_albums,
    device_authorization, device_authorization_token, favorite_albums, favorite_artists,
    favorite_tracks, models::TidalAlbum, remove_favorite_album, remove_favorite_artist,
    remove_favorite_track, search, track, track_file_url, track_playback_info,
};

/// Binds all Tidal API endpoints to an Actix Web service scope.
///
/// This function registers all available Tidal endpoints including authentication,
/// favorites management, track retrieval, and search functionality.
#[must_use]
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
/// `OpenAPI` documentation configuration for Tidal API endpoints.
pub struct Api;

/// Tidal album representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    /// Tidal album ID.
    pub id: u64,
    /// Album artist name.
    pub artist: String,
    /// Tidal artist ID.
    pub artist_id: u64,
    /// Album type (LP, EPs/Singles, Compilations).
    pub album_type: TidalAlbumType,
    /// Whether the album has cover artwork available.
    pub contains_cover: bool,
    /// Audio quality level for this album.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album contains explicit content.
    pub explicit: bool,
    /// Total number of tracks on the album.
    pub number_of_tracks: u32,
    /// Album popularity score.
    pub popularity: u32,
    /// Release date in ISO 8601 format.
    pub date_released: Option<String>,
    /// Album title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
    /// API source identifier.
    pub api_source: ApiSource,
}

/// Track representation for API responses supporting different music sources.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    /// Track from the Tidal music service.
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
            api_source: API_SOURCE.clone(),
        })
    }
}

/// Tidal track representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalTrack {
    /// Tidal track ID.
    pub id: u64,
    /// Track number on the album.
    pub number: u32,
    /// Album title.
    pub album: String,
    /// Tidal album ID.
    pub album_id: u64,
    /// Album type (LP, EPs/Singles, Compilations).
    pub album_type: TidalAlbumType,
    /// Artist name.
    pub artist: String,
    /// Tidal artist ID.
    pub artist_id: u64,
    /// Whether the track has cover artwork available.
    pub contains_cover: bool,
    /// Audio quality level for this track.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Track duration in seconds.
    pub duration: u32,
    /// Whether the track contains explicit content.
    pub explicit: bool,
    /// International Standard Recording Code.
    pub isrc: String,
    /// Track popularity score.
    pub popularity: u32,
    /// Track title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
    /// API source identifier.
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
            track_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

/// Tidal artist representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalArtist {
    /// Tidal artist ID.
    pub id: u64,
    /// Whether the artist has cover artwork available.
    pub contains_cover: bool,
    /// Artist popularity score.
    pub popularity: u32,
    /// Artist name.
    pub title: String,
    /// API source identifier.
    pub api_source: ApiSource,
}

impl From<Error> for actix_web::Error {
    fn from(e: Error) -> Self {
        match &e {
            Error::Unauthorized => {
                return ErrorUnauthorized(e.to_string());
            }
            Error::HttpRequestFailed(status, message) => {
                if *status == 404 {
                    return ErrorNotFound(format!("Tidal album not found: {message}"));
                }
            }
            Error::NoUserIdAvailable
            | Error::Parse(..)
            | Error::Http(..)
            | Error::NoAccessTokenAvailable
            | Error::RequestFailed(..)
            | Error::MaxFailedAttempts
            | Error::NoResponseBody
            | Error::Serde(..) => {}
            #[cfg(feature = "db")]
            Error::Database(..) | Error::TidalConfig(..) => {}
        }

        log::error!("{e:?}");
        ErrorInternalServerError(e.to_string())
    }
}

static TIDAL_ACCESS_TOKEN_HEADER: &str = "x-tidal-access-token";

/// Query parameters for initiating Tidal device authorization.
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
/// Initiates Tidal device authorization flow.
///
/// Starts the OAuth 2.0 device authorization flow, returning a verification URL
/// and device code for the user to complete authorization.
///
/// # Errors
///
/// * `ErrorInternalServerError` - If the HTTP request to Tidal's authorization endpoint fails
pub async fn device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization(query.client_id.clone(), query.open.unwrap_or(false)).await?,
    ))
}

/// Query parameters for exchanging device code for access token.
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
/// Completes Tidal device authorization by exchanging device code for access token.
///
/// Exchanges the device code from the authorization flow for an access token and
/// optionally persists the credentials to the database.
///
/// # Errors
///
/// * `ErrorInternalServerError` - If the token exchange request fails or database persistence fails
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

/// Query parameters for fetching track playback URLs.
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
/// Retrieves the playback URL for a Tidal track.
///
/// Returns the streaming URL for the specified track at the requested audio quality level.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the track ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching track playback information.
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
/// Retrieves detailed playback information for a Tidal track.
///
/// Returns metadata needed for track playback including audio quality, format, and streaming details.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the track ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching favorite artists.
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
/// Fetches the user's favorite artists from Tidal.
///
/// Returns a paginated list of artists the authenticated user has marked as favorites.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for adding an artist to favorites.
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
/// Adds an artist to the user's Tidal favorites.
///
/// Marks the specified artist as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the artist ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for removing an artist from favorites.
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
/// Removes an artist from the user's Tidal favorites.
///
/// Unmarks the specified artist as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the artist ID does not exist or is not in favorites
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching favorite albums.
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
/// Fetches the user's favorite albums from Tidal.
///
/// Returns a paginated list of albums the authenticated user has marked as favorites.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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
                .map_err(|e| Error::RequestFailed(format!("{e:?}")))
                as Result<ApiAlbum, Error>
        })
        .transpose()
        .map_err(ErrorInternalServerError)?
        .into(),
    ))
}

/// Query parameters for adding an album to favorites.
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
/// Adds an album to the user's Tidal favorites.
///
/// Marks the specified album as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the album ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for removing an album from favorites.
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
/// Removes an album from the user's Tidal favorites.
///
/// Unmarks the specified album as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the album ID does not exist or is not in favorites
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for adding a track to favorites.
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
/// Adds a track to the user's Tidal favorites.
///
/// Marks the specified track as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the track ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for removing a track from favorites.
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
/// Removes a track from the user's Tidal favorites.
///
/// Unmarks the specified track as a favorite for the authenticated user.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the track ID does not exist or is not in favorites
/// * `ErrorInternalServerError` - If the request to Tidal fails
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching favorite tracks.
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
/// Fetches the user's favorite tracks from Tidal.
///
/// Returns a paginated list of tracks the authenticated user has marked as favorites.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching albums by an artist.
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

/// Album type filter for artist album queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    /// Full-length studio album (LP).
    Lp,
    /// Extended plays (EPs) and single releases.
    EpsAndSingles,
    /// Compilation albums and collections.
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
/// Fetches albums by a specific artist from Tidal.
///
/// Returns a paginated list of albums for the specified artist.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the artist ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching tracks from an album.
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
/// Fetches tracks from a specific album on Tidal.
///
/// Returns a paginated list of tracks for the specified album.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the album ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching album metadata.
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
/// Fetches metadata for a specific album from Tidal.
///
/// Returns detailed information about the specified album including title, artist, tracks count, and quality.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the album ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching artist metadata.
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
/// Fetches metadata for a specific artist from Tidal.
///
/// Returns detailed information about the specified artist including name, popularity, and cover art.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the artist ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for fetching track metadata.
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
/// Fetches metadata for a specific track from Tidal.
///
/// Returns detailed information about the specified track including title, artist, album, duration, and quality.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorNotFound` - If the track ID does not exist
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

/// Query parameters for searching Tidal content.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchQuery {
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
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
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
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
/// Searches Tidal's catalog for artists, albums, and tracks.
///
/// Performs a fuzzy search across Tidal's library and returns matching artists, albums, and tracks.
/// Requires authentication via access token in the request header.
///
/// # Errors
///
/// * `ErrorUnauthorized` - If no valid access token is provided
/// * `ErrorInternalServerError` - If the request to Tidal fails or the response cannot be parsed
///
/// # Panics
///
/// * If the access token header contains non-UTF8 data
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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_json_utils::ParseError;
    use moosicbox_music_models::id::Id;
    use pretty_assertions::assert_eq;

    // Error conversion tests
    #[test_log::test]
    fn test_error_to_actix_error_unauthorized() {
        let error = Error::Unauthorized;
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[test_log::test]
    fn test_error_to_actix_error_http_request_failed_404() {
        let error = Error::HttpRequestFailed(404, "Not found".to_string());
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test_log::test]
    fn test_error_to_actix_error_http_request_failed_non_404() {
        let error = Error::HttpRequestFailed(500, "Server error".to_string());
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_user_id_available() {
        let error = Error::NoUserIdAvailable;
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_access_token_available() {
        let error = Error::NoAccessTokenAvailable;
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_parse() {
        let error = Error::Parse(ParseError::MissingValue("test".to_string()));
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_max_failed_attempts() {
        let error = Error::MaxFailedAttempts;
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_response_body() {
        let error = Error::NoResponseBody;
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_request_failed() {
        let error = Error::RequestFailed("Some error".to_string());
        let actix_error: actix_web::Error = error.into();
        let response = actix_error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    // AlbumType conversion tests
    #[test_log::test]
    fn test_album_type_to_tidal_album_type_lp() {
        let api_album_type = AlbumType::Lp;
        let tidal_album_type: TidalAlbumType = api_album_type.into();
        assert_eq!(tidal_album_type, TidalAlbumType::Lp);
    }

    #[test_log::test]
    fn test_album_type_to_tidal_album_type_eps_and_singles() {
        let api_album_type = AlbumType::EpsAndSingles;
        let tidal_album_type: TidalAlbumType = api_album_type.into();
        assert_eq!(tidal_album_type, TidalAlbumType::EpsAndSingles);
    }

    #[test_log::test]
    fn test_album_type_to_tidal_album_type_compilations() {
        let api_album_type = AlbumType::Compilations;
        let tidal_album_type: TidalAlbumType = api_album_type.into();
        assert_eq!(tidal_album_type, TidalAlbumType::Compilations);
    }

    // TidalTrack to ApiTrack conversion tests
    #[test_log::test]
    fn test_tidal_track_to_api_track_with_album_cover() {
        let tidal_track = TidalTrack {
            id: 12345,
            track_number: 5,
            artist_id: 67890,
            artist: "Test Artist".to_string(),
            artist_cover: None,
            album_id: 11111,
            album_type: TidalAlbumType::Lp,
            album: "Test Album".to_string(),
            album_cover: Some("cover-hash".to_string()),
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Test".to_string()),
            duration: 240,
            explicit: true,
            isrc: "USRC12345678".to_string(),
            popularity: 85,
            title: "Test Track".to_string(),
            media_metadata_tags: vec!["LOSSLESS".to_string()],
        };

        let api_track: ApiTrack = tidal_track.into();
        match api_track {
            ApiTrack::Tidal(track) => {
                assert_eq!(track.id, 12345);
                assert_eq!(track.number, 5);
                assert_eq!(track.artist, "Test Artist");
                assert_eq!(track.artist_id, 67890);
                assert_eq!(track.album, "Test Album");
                assert_eq!(track.album_id, 11111);
                assert_eq!(track.album_type, TidalAlbumType::Lp);
                assert!(track.contains_cover);
                assert_eq!(track.audio_quality, "LOSSLESS");
                assert_eq!(track.copyright, Some("2024 Test".to_string()));
                assert_eq!(track.duration, 240);
                assert!(track.explicit);
                assert_eq!(track.isrc, "USRC12345678");
                assert_eq!(track.popularity, 85);
                assert_eq!(track.title, "Test Track");
                assert_eq!(track.media_metadata_tags, vec!["LOSSLESS".to_string()]);
            }
        }
    }

    #[test_log::test]
    fn test_tidal_track_to_api_track_without_album_cover() {
        let tidal_track = TidalTrack {
            id: 12345,
            track_number: 1,
            artist_id: 67890,
            artist: "Test Artist".to_string(),
            artist_cover: None,
            album_id: 11111,
            album_type: TidalAlbumType::EpsAndSingles,
            album: "Test Single".to_string(),
            album_cover: None,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 180,
            explicit: false,
            isrc: "USRC87654321".to_string(),
            popularity: 50,
            title: "Test Single Track".to_string(),
            media_metadata_tags: vec![],
        };

        let api_track: ApiTrack = tidal_track.into();
        match api_track {
            ApiTrack::Tidal(track) => {
                assert!(!track.contains_cover);
                assert_eq!(track.album_type, TidalAlbumType::EpsAndSingles);
                assert!(track.copyright.is_none());
                assert!(!track.explicit);
                assert!(track.media_metadata_tags.is_empty());
            }
        }
    }

    // ApiTidalTrack to moosicbox_music_models::api::ApiTrack conversion tests
    #[test_log::test]
    fn test_api_tidal_track_to_api_track_conversion() {
        let api_tidal_track = ApiTidalTrack {
            id: 98765,
            number: 3,
            album: "Test Album".to_string(),
            album_id: 54321,
            album_type: TidalAlbumType::Lp,
            artist: "Test Artist".to_string(),
            artist_id: 11111,
            contains_cover: true,
            audio_quality: "HI_RES_LOSSLESS".to_string(),
            copyright: Some("2024 Test Corp".to_string()),
            duration: 300,
            explicit: false,
            isrc: "GBRC12345678".to_string(),
            popularity: 75,
            title: "Test Track Title".to_string(),
            media_metadata_tags: vec!["HIRES_LOSSLESS".to_string()],
            api_source: API_SOURCE.clone(),
        };

        let api_track: moosicbox_music_models::api::ApiTrack = api_tidal_track.into();
        assert_eq!(api_track.track_id, Id::from(98765_u64));
        assert_eq!(api_track.number, 3);
        assert_eq!(api_track.title, "Test Track Title");
        assert!((api_track.duration - 300.0).abs() < f64::EPSILON);
        assert_eq!(api_track.album, "Test Album");
        assert_eq!(api_track.album_id, Id::from(54321_u64));
        assert_eq!(api_track.artist, "Test Artist");
        assert_eq!(api_track.artist_id, Id::from(11111_u64));
        assert!(api_track.contains_cover);
        assert!(!api_track.blur);
        assert!(api_track.format.is_none());
        assert!(api_track.bit_depth.is_none());
        assert!(api_track.sample_rate.is_none());
        assert!(api_track.channels.is_none());
        assert_eq!(api_track.api_source, *API_SOURCE);
    }

    #[test_log::test]
    fn test_api_tidal_track_to_api_track_default_values() {
        let api_tidal_track = ApiTidalTrack::default();

        let api_track: moosicbox_music_models::api::ApiTrack = api_tidal_track.into();
        assert_eq!(api_track.track_id, Id::from(0_u64));
        assert_eq!(api_track.number, 0);
        assert!(api_track.title.is_empty());
        assert!((api_track.duration - 0.0).abs() < f64::EPSILON);
        assert!(api_track.album.is_empty());
        assert!(api_track.artist.is_empty());
        assert!(!api_track.contains_cover);
        assert!(api_track.date_released.is_none());
        assert!(api_track.date_added.is_none());
    }
}
