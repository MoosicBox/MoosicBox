#![allow(clippy::needless_for_each)]
#![allow(clippy::module_name_repetitions, clippy::future_not_send)]

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
use switchy_database::profiles::LibraryDatabase;

use crate::{
    API_SOURCE, Error, YtAlbumOrder, YtAlbumOrderDirection, YtAlbumType, YtArtistOrder,
    YtArtistOrderDirection, YtAudioQuality, YtDeviceType, YtTrack, YtTrackOrder,
    YtTrackOrderDirection, YtTrackPlaybackInfo, add_favorite_album, add_favorite_artist,
    add_favorite_track, album, album_tracks, artist, artist_albums, device_authorization,
    device_authorization_token, favorite_albums, favorite_artists, favorite_tracks,
    remove_favorite_album, remove_favorite_artist, remove_favorite_track, search, track,
    track_file_url, track_playback_info,
};

/// Binds all `YouTube` Music API endpoints to the provided Actix-web scope.
///
/// Registers routes for device authorization, favorites, tracks, albums, artists, and search.
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

/// `OpenAPI` specification for `YouTube` Music API endpoints.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "YouTube Music")),
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
        YtTrackPlaybackInfo,
        YtDeviceType,
        YtAudioQuality,
        YtArtistOrder,
        YtArtistOrderDirection,
        YtAlbumOrder,
        YtAlbumOrderDirection,
        YtTrackOrder,
        YtTrackOrderDirection,
    ))
)]
/// Marker struct for the `YouTube` Music API routes.
pub struct Api;

/// `YouTube` Music album representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtAlbum {
    /// `YouTube` Music album ID
    pub id: String,
    /// Album artist name
    pub artist: String,
    /// `YouTube` Music artist ID
    pub artist_id: String,
    /// Whether the album has cover artwork
    pub contains_cover: bool,
    /// Audio quality level (e.g., "HIGH", "LOSSLESS")
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Total album duration in seconds
    pub duration: u32,
    /// Whether the album contains explicit content
    pub explicit: bool,
    /// Number of tracks in the album
    pub number_of_tracks: u32,
    /// Album popularity score
    pub popularity: u32,
    /// Release date (ISO 8601 format)
    pub date_released: Option<String>,
    /// Album title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
    /// API source identifier
    pub api_source: ApiSource,
}

/// Track type wrapper for API responses.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    /// `YouTube` Music track
    Yt(ApiYtTrack),
}

impl From<YtTrack> for ApiTrack {
    fn from(value: YtTrack) -> Self {
        Self::Yt(ApiYtTrack {
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

impl From<ApiTrack> for moosicbox_music_models::api::ApiTrack {
    fn from(value: ApiTrack) -> Self {
        let ApiTrack::Yt(track) = value;
        track.into()
    }
}

/// `YouTube` Music track representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtTrack {
    /// `YouTube` Music track ID
    pub id: String,
    /// Track number within the album
    pub number: u32,
    /// Album title
    pub album: String,
    /// `YouTube` Music album ID
    pub album_id: String,
    /// Album type classification
    pub album_type: YtAlbumType,
    /// Artist name
    pub artist: String,
    /// `YouTube` Music artist ID
    pub artist_id: String,
    /// Whether the track has cover artwork
    pub contains_cover: bool,
    /// Audio quality level (e.g., "HIGH", "LOSSLESS")
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Track duration in seconds
    pub duration: u32,
    /// Whether the track contains explicit content
    pub explicit: bool,
    /// International Standard Recording Code
    pub isrc: String,
    /// Track popularity score
    pub popularity: u32,
    /// Track title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
    /// API source identifier
    pub api_source: ApiSource,
}

impl From<ApiYtTrack> for moosicbox_music_models::api::ApiTrack {
    fn from(value: ApiYtTrack) -> Self {
        Self {
            track_id: value.id.clone().into(),
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

/// `YouTube` Music artist representation for API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtArtist {
    /// `YouTube` Music artist ID
    pub id: String,
    /// Whether the artist has a cover image
    pub contains_cover: bool,
    /// Artist popularity score
    pub popularity: u32,
    /// Artist name
    pub title: String,
    /// API source identifier
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
            | Error::EmptyResponse
            | Error::Config(..)
            | Error::Serde(..) => {}
            #[cfg(feature = "db")]
            Error::Database(..) | Error::YtConfig(..) => {}
        }

        log::error!("{e:?}");
        ErrorInternalServerError(e.to_string())
    }
}

static YT_ACCESS_TOKEN_HEADER: &str = "x-yt-access-token";

/// Query parameters for device authorization endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDeviceAuthorizationQuery {
    client_id: String,
    open: Option<bool>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        post,
        path = "/auth/device-authorization",
        description = "Begin the authorization process for YouTube Music",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("client_id" = String, Query, description = "YouTube Music client ID to use"),
            ("open" = Option<bool>, Query, description = "Open the authorization page in a browser"),
        ),
        responses(
            (
                status = 200,
                description = "URL and Device code used in the YouTube Music authorization flow",
                body = Value,
            )
        )
    )
)]
#[route("/auth/device-authorization", method = "POST")]
pub async fn device_authorization_endpoint(
    query: web::Query<YtDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization(query.client_id.clone(), query.open.unwrap_or(false)).await?,
    ))
}

/// Query parameters for device authorization token endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDeviceAuthorizationTokenQuery {
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")]
    persist: Option<bool>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        post,
        path = "/auth/device-authorization/token",
        description = "Finish the authorization process for YouTube Music",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("client_id" = String, Query, description = "YouTube Music client ID to use"),
            ("client_secret" = String, Query, description = "YouTube Music client secret to use"),
            ("device_code" = String, Query, description = "YouTube Music device code to use"),
        ),
        responses(
            (
                status = 200,
                description = "Access token and refresh token used in the YouTube Music authentication",
                body = Value,
            )
        )
    )
)]
#[route("/auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    query: web::Query<YtDeviceAuthorizationTokenQuery>,
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

/// Query parameters for track file URL endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtTrackFileUrlQuery {
    audio_quality: YtAudioQuality,
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/track/url",
        description = "Get YouTube Music track file stream URL",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("audioQuality" = YtAudioQuality, Query, description = "Audio quality to fetch the file stream for"),
            ("trackId" = u64, Query, description = "YouTube Music track ID to fetch track stream URL for"),
        ),
        responses(
            (
                status = 200,
                description = "YouTube Music track URL for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackFileUrlQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            #[cfg(feature = "db")]
            &db,
            query.audio_quality,
            &query.track_id.into(),
            req.headers()
                .get(YT_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    })))
}

/// Query parameters for track playback info endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtTrackPlaybackInfoQuery {
    audio_quality: YtAudioQuality,
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/track/playback-info",
        description = "Get YouTube Music track metadata info",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("audioQuality" = YtAudioQuality, Query, description = "Audio quality to fetch the track metadata for"),
            ("trackId" = u64, Query, description = "YouTube Music track ID to fetch track metadata for"),
        ),
        responses(
            (
                status = 200,
                description = "YouTube Music track metadata info",
                body = YtTrackPlaybackInfo,
            )
        )
    )
)]
#[route("/track/playback-info", method = "GET")]
pub async fn track_playback_info_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackPlaybackInfoQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<YtTrackPlaybackInfo>> {
    Ok(Json(
        track_playback_info(
            #[cfg(feature = "db")]
            &db,
            query.audio_quality,
            &query.track_id.into(),
            req.headers()
                .get(YT_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}

/// Query parameters for favorite artists endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtFavoriteArtistsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtArtistOrder>,
    order_direction: Option<YtArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/favorites/artists",
        description = "Get YouTube Music favorited artists",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<YtArtistOrder>, Query, description = "Sort property to sort the artists by"),
            ("orderDirection" = Option<YtAlbumOrderDirection>, Query, description = "Sort order direction to order the artists by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of YouTube Music favorited artists",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteArtistsQuery>,
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
                .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtAddFavoriteArtistsQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        post,
        path = "/favorites/artists",
        description = "Favorite a YouTube Music artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "YouTube Music artist ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn add_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteArtistsQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtRemoveFavoriteArtistsQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        delete,
        path = "/favorites/artists",
        description = "Remove YouTube Music artist from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "YouTube Music artist ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn remove_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteArtistsQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

/// Query parameters for favorite albums endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtAlbumOrder>,
    order_direction: Option<YtAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/favorites/albums",
        description = "Get YouTube Music favorited albums",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<YtAlbumOrder>, Query, description = "Sort property to sort the albums by"),
            ("orderDirection" = Option<YtAlbumOrderDirection>, Query, description = "Sort order direction to order the albums by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of YouTube Music favorited albums",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteAlbumsQuery>,
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
                .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtAddFavoriteAlbumsQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        post,
        path = "/favorites/albums",
        description = "Favorite a YouTube Music album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "YouTube Music album ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn add_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteAlbumsQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtRemoveFavoriteAlbumsQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        delete,
        path = "/favorites/albums",
        description = "Remove YouTube Music album from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "YouTube Music album ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn remove_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteAlbumsQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtAddFavoriteTracksQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        post,
        path = "/favorites/tracks",
        description = "Favorite a YouTube Music track",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "YouTube Music track ID to favorite"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn add_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteTracksQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
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
pub struct YtRemoveFavoriteTracksQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        delete,
        path = "/favorites/tracks",
        description = "Remove YouTube Music track from favorites",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "YouTube Music track ID to remove from favorites"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
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
pub async fn remove_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteTracksQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

/// Query parameters for favorite tracks endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtFavoriteTracksQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtTrackOrder>,
    order_direction: Option<YtTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    user_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/favorites/tracks",
        description = "Get YouTube Music favorited tracks",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<YtTrackOrder>, Query, description = "Sort property to sort the tracks by"),
            ("orderDirection" = Option<YtTrackOrderDirection>, Query, description = "Sort order direction to order the tracks by"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
            ("userId" = Option<u64>, Query, description = "User ID making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of YouTube Music favorited tracks",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<YtTrack> = favorite_tracks(
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?
    .into();

    Ok(Json(tracks.into()))
}

/// Query parameters for artist albums endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistAlbumsQuery {
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<AlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

/// Album type classification for API queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    /// Full-length album
    Lp,
    /// EPs and singles
    EpsAndSingles,
    /// Compilation albums
    Compilations,
}

impl From<AlbumType> for YtAlbumType {
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
        tags = ["YouTube Music"],
        get,
        path = "/artists/albums",
        description = "Get YouTube Music albums for the specified artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "YouTube Music artist ID to search for albums for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("albumType" = Option<AlbumType>, Query, description = "Album type to filter to"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of YouTube Music albums for an artist",
                body = Value,
            )
        )
    )
)]
#[route("/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<YtArtistAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        artist_albums(
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
                .get(YT_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
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

/// Query parameters for album tracks endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtAlbumTracksQuery {
    album_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/albums/tracks",
        description = "Get YouTube Music tracks for the specified album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "YouTube Music album ID to search for tracks for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "Page of YouTube Music tracks for an album",
                body = Value,
            )
        )
    )
)]
#[route("/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<YtAlbumTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<YtTrack> = album_tracks(
        #[cfg(feature = "db")]
        &db,
        &query.album_id.into(),
        query.offset,
        query.limit,
        query.country_code.clone(),
        query.locale.clone(),
        query.device_type,
        req.headers()
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    Ok(Json(tracks.into()))
}

/// Query parameters for album endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtAlbumQuery {
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/albums",
        description = "Get YouTube Music album for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "YouTube Music album ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "YouTube Music album for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/albums", method = "GET")]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<YtAlbumQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.try_into().map_err(ErrorInternalServerError)?))
}

/// Query parameters for artist endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistQuery {
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/artists",
        description = "Get YouTube Music artist for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "YouTube Music artist ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "YouTube Music artist for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtArtistQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.into()))
}

/// Query parameters for track endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtTrackQuery {
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/tracks",
        description = "Get YouTube Music track for the specified ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "YouTube Music track ID to fetch"),
            ("countryCode" = Option<String>, Query, description = "Country code to request from"),
            ("locale" = Option<String>, Query, description = "Locale to request with"),
            ("deviceType" = Option<YtDeviceType>, Query, description = "Device type making the request"),
        ),
        responses(
            (
                status = 200,
                description = "YouTube Music track for the specified ID",
                body = Value,
            )
        )
    )
)]
#[route("/tracks", method = "GET")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackQuery>,
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
            .get(YT_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.into()))
}

/// Query parameters for search endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchQuery {
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/search",
        description = "Search the YouTube Music library for artists/albums/tracks that fuzzy match the query",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("query" = String, Query, description = "The search query"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
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
pub async fn search_endpoint(
    query: web::Query<YtSearchQuery>,
) -> Result<Json<ApiSearchResultsResponse>> {
    Ok(Json(
        search(&query.query, query.offset, query.limit)
            .await?
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_yt_track_to_api_track_with_album_cover() {
        let yt_track = YtTrack {
            id: "track123".to_string(),
            track_number: 5,
            artist_id: "artist456".to_string(),
            artist: "Test Artist".to_string(),
            artist_cover: Some("artist-cover-url".to_string()),
            album_id: "album789".to_string(),
            album: "Test Album".to_string(),
            album_type: YtAlbumType::Lp,
            album_cover: Some("album-cover-url".to_string()),
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Test Label".to_string()),
            duration: 180,
            explicit: true,
            isrc: "USABC1234567".to_string(),
            popularity: 85,
            title: "Test Track".to_string(),
            media_metadata_tags: vec!["tag1".to_string(), "tag2".to_string()],
        };

        let api_track: ApiTrack = yt_track.into();
        match api_track {
            ApiTrack::Yt(track) => {
                assert_eq!(track.id, "track123");
                assert_eq!(track.number, 5);
                assert_eq!(track.album, "Test Album");
                assert_eq!(track.album_id, "album789");
                assert_eq!(track.album_type, YtAlbumType::Lp);
                assert_eq!(track.artist, "Test Artist");
                assert_eq!(track.artist_id, "artist456");
                assert!(track.contains_cover);
                assert_eq!(track.audio_quality, "LOSSLESS");
                assert_eq!(track.copyright, Some("2024 Test Label".to_string()));
                assert_eq!(track.duration, 180);
                assert!(track.explicit);
                assert_eq!(track.isrc, "USABC1234567");
                assert_eq!(track.popularity, 85);
                assert_eq!(track.title, "Test Track");
                assert_eq!(track.media_metadata_tags, vec!["tag1", "tag2"]);
            }
        }
    }

    #[test_log::test]
    fn test_yt_track_to_api_track_without_album_cover() {
        let yt_track = YtTrack {
            id: "track456".to_string(),
            track_number: 1,
            artist_id: "artist111".to_string(),
            artist: "Another Artist".to_string(),
            artist_cover: None,
            album_id: "album222".to_string(),
            album: "Another Album".to_string(),
            album_type: YtAlbumType::EpsAndSingles,
            album_cover: None,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 240,
            explicit: false,
            isrc: "USXYZ9876543".to_string(),
            popularity: 60,
            title: "Another Track".to_string(),
            media_metadata_tags: vec![],
        };

        let api_track: ApiTrack = yt_track.into();
        match api_track {
            ApiTrack::Yt(track) => {
                assert!(!track.contains_cover);
                assert_eq!(track.copyright, None);
                assert!(!track.explicit);
                assert!(track.media_metadata_tags.is_empty());
            }
        }
    }

    #[test_log::test]
    fn test_api_track_to_music_models_api_track() {
        let api_yt_track = ApiYtTrack {
            id: "track789".to_string(),
            number: 3,
            album: "Album Name".to_string(),
            album_id: "album_id_123".to_string(),
            album_type: YtAlbumType::Compilations,
            artist: "Artist Name".to_string(),
            artist_id: "artist_id_456".to_string(),
            contains_cover: true,
            audio_quality: "HI_RES_LOSSLESS".to_string(),
            copyright: Some("Copyright Info".to_string()),
            duration: 300,
            explicit: true,
            isrc: "ISRC123456789".to_string(),
            popularity: 95,
            title: "Track Title".to_string(),
            media_metadata_tags: vec!["lossless".to_string()],
            api_source: API_SOURCE.clone(),
        };

        let api_track = ApiTrack::Yt(api_yt_track);
        let music_api_track: moosicbox_music_models::api::ApiTrack = api_track.into();

        assert_eq!(music_api_track.number, 3);
        assert_eq!(music_api_track.title, "Track Title");
        assert!((music_api_track.duration - 300.0).abs() < f64::EPSILON);
        assert_eq!(music_api_track.album, "Album Name");
        assert_eq!(music_api_track.artist, "Artist Name");
        assert!(music_api_track.contains_cover);
        assert!(!music_api_track.blur);
        assert!(music_api_track.date_released.is_none());
        assert!(music_api_track.date_added.is_none());
        assert!(music_api_track.format.is_none());
        assert!(music_api_track.bit_depth.is_none());
    }

    #[test_log::test]
    fn test_api_yt_track_to_music_models_api_track_directly() {
        let api_yt_track = ApiYtTrack {
            id: "direct_track".to_string(),
            number: 7,
            album: "Direct Album".to_string(),
            album_id: "direct_album_id".to_string(),
            album_type: YtAlbumType::Lp,
            artist: "Direct Artist".to_string(),
            artist_id: "direct_artist_id".to_string(),
            contains_cover: false,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            duration: 210,
            explicit: false,
            isrc: "DIRECT123".to_string(),
            popularity: 70,
            title: "Direct Track Title".to_string(),
            media_metadata_tags: vec![],
            api_source: API_SOURCE.clone(),
        };

        let music_api_track: moosicbox_music_models::api::ApiTrack = api_yt_track.into();

        assert_eq!(music_api_track.number, 7);
        assert_eq!(music_api_track.title, "Direct Track Title");
        assert!((music_api_track.duration - 210.0).abs() < f64::EPSILON);
        assert!(!music_api_track.contains_cover);
    }

    #[test_log::test]
    fn test_api_album_type_to_yt_album_type() {
        assert_eq!(YtAlbumType::from(AlbumType::Lp), YtAlbumType::Lp);
        assert_eq!(
            YtAlbumType::from(AlbumType::EpsAndSingles),
            YtAlbumType::EpsAndSingles
        );
        assert_eq!(
            YtAlbumType::from(AlbumType::Compilations),
            YtAlbumType::Compilations
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_unauthorized_returns_401() {
        let error: actix_web::Error = Error::Unauthorized.into();
        let response = error.error_response();
        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[test_log::test]
    fn test_error_to_actix_error_http_404_returns_not_found() {
        let error: actix_web::Error =
            Error::HttpRequestFailed(404, "Album not found".to_string()).into();
        let response = error.error_response();
        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test_log::test]
    fn test_error_to_actix_error_http_500_returns_internal_server_error() {
        let error: actix_web::Error =
            Error::HttpRequestFailed(500, "Server error".to_string()).into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_http_403_returns_internal_server_error() {
        let error: actix_web::Error = Error::HttpRequestFailed(403, "Forbidden".to_string()).into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_user_id_returns_internal_server_error() {
        let error: actix_web::Error = Error::NoUserIdAvailable.into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_access_token_returns_internal_server_error() {
        let error: actix_web::Error = Error::NoAccessTokenAvailable.into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_max_failed_attempts_returns_internal_server_error() {
        let error: actix_web::Error = Error::MaxFailedAttempts.into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_no_response_body_returns_internal_server_error() {
        let error: actix_web::Error = Error::NoResponseBody.into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_empty_response_returns_internal_server_error() {
        let error: actix_web::Error = Error::EmptyResponse.into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_error_to_actix_error_request_failed_returns_internal_server_error() {
        let error: actix_web::Error = Error::RequestFailed("Request failed".to_string()).into();
        let response = error.error_response();
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
