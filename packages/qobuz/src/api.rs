//! HTTP API endpoints for Qobuz music service integration.
//!
//! This module provides actix-web route handlers for interacting with the Qobuz API,
//! including authentication, browsing artists/albums/tracks, managing favorites,
//! and searching the Qobuz catalog.
//!
//! Use [`bind_services`](crate::api::bind_services) to register all endpoints with an actix-web scope.

#![allow(clippy::needless_for_each)]
#![allow(clippy::future_not_send, clippy::module_name_repetitions)]

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
    API_SOURCE, Error, QobuzAlbumOrder, QobuzAlbumReleaseType, QobuzAlbumSort, QobuzAudioQuality,
    QobuzRelease, QobuzTrack, album, album_tracks, artist, artist_albums, favorite_albums,
    favorite_artists, favorite_tracks, format_title, models::QobuzAlbum, search, track,
    track_file_url, user_login,
};

/// Binds all Qobuz API endpoints to an actix-web scope.
///
/// This function registers route handlers for authentication, favorites, albums, artists,
/// tracks, and search operations.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(user_login_endpoint)
        .service(track_file_url_endpoint)
        .service(favorite_artists_endpoint)
        .service(favorite_albums_endpoint)
        .service(favorite_tracks_endpoint)
        .service(artist_albums_endpoint)
        .service(album_tracks_endpoint)
        .service(album_endpoint)
        .service(artist_endpoint)
        .service(track_endpoint)
        .service(search_endpoint)
}

/// `OpenAPI` documentation for Qobuz API endpoints.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Qobuz")),
    paths(
        user_login_endpoint,
        artist_endpoint,
        favorite_artists_endpoint,
        album_endpoint,
        artist_albums_endpoint,
        favorite_albums_endpoint,
        album_tracks_endpoint,
        track_endpoint,
        favorite_tracks_endpoint,
        track_file_url_endpoint,
        search_endpoint,
    ),
    components(schemas(
        AlbumReleaseType,
        AlbumSort,
        AlbumOrder,
        QobuzAudioQuality,
    ))
)]
pub struct Api;

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
            | Error::NoAppId
            | Error::NoSeedAndTimezone
            | Error::NoInfoAndExtras
            | Error::NoMatchingInfoForTimezone
            | Error::Utf8(..)
            | Error::FailedToFetchAppId
            | Error::NoAppSecretAvailable
            | Error::Base64Decode(..)
            | Error::Config(..)
            | Error::Serde(..) => {}
            #[cfg(feature = "db")]
            Error::Database(_) | Error::DatabaseFetch(_) => {}
        }

        log::error!("{e:?}");
        ErrorInternalServerError(e.to_string())
    }
}

/// Query parameters for authenticating a user with the Qobuz API.
///
/// Used by the user login endpoint to obtain access tokens for subsequent API requests.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzUserLoginQuery {
    /// Qobuz account username or email.
    username: String,
    /// Qobuz account password.
    password: String,
    /// Whether to persist credentials to database (requires `db` feature).
    #[cfg(feature = "db")]
    persist: Option<bool>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        post,
        path = "/auth/login",
        description = "Login to Qobuz",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("username" = String, Query, description = "Qobuz login username"),
            ("password" = String, Query, description = "Qobuz login password"),
        ),
        responses(
            (
                status = 200,
                description = "Access token credentials",
                body = Value,
            )
        )
    )
)]
#[route("/auth/login", method = "POST")]
/// # Panics
///
/// * If the `x-qobuz-app-id` header contains invalid UTF-8 bytes
pub async fn user_login_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzUserLoginQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(
        user_login(
            #[cfg(feature = "db")]
            &db,
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

/// API response type for Qobuz album with simplified metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiQobuzAlbum {
    /// Album identifier.
    pub id: String,
    /// Primary artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Release type.
    pub album_type: QobuzAlbumReleaseType,
    /// Whether album artwork is available.
    pub contains_cover: bool,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album has explicit content.
    pub parental_warning: bool,
    /// Number of tracks on the album.
    pub number_of_tracks: u32,
    /// Release date as string.
    pub date_released: String,
    /// Album title.
    pub title: String,
    /// Source API identifier.
    pub api_source: ApiSource,
}

/// API response type for Qobuz album release with simplified metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiQobuzRelease {
    /// Album identifier.
    pub id: String,
    /// Primary artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Release type.
    pub album_type: QobuzAlbumReleaseType,
    /// Whether album artwork is available.
    pub contains_cover: bool,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the release has explicit content.
    pub parental_warning: bool,
    /// Number of tracks on the release.
    pub number_of_tracks: u32,
    /// Release date as string.
    pub date_released: String,
    /// Album title.
    pub title: String,
    /// Source API identifier.
    pub api_source: ApiSource,
}

/// Tagged union for API album releases from different sources.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiRelease {
    /// Qobuz album release.
    Qobuz(ApiQobuzRelease),
}

impl From<QobuzRelease> for ApiRelease {
    fn from(value: QobuzRelease) -> Self {
        Self::Qobuz(ApiQobuzRelease {
            contains_cover: value.cover_url().is_some(),
            id: value.id,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            duration: value.duration,
            title: format_title(&value.title, value.version.as_deref()),
            parental_warning: value.parental_warning,
            number_of_tracks: value.tracks_count,
            date_released: value.release_date_original,
            api_source: API_SOURCE.clone(),
        })
    }
}

/// Tagged union for API tracks from different sources.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    /// Qobuz track.
    Qobuz(ApiQobuzTrack),
}

impl From<QobuzTrack> for ApiTrack {
    fn from(value: QobuzTrack) -> Self {
        Self::Qobuz(ApiQobuzTrack {
            contains_cover: value.cover_url().is_some(),
            id: value.id,
            number: value.track_number,
            artist: value.artist,
            artist_id: value.artist_id,
            album: value.album,
            album_id: value.album_id,
            album_type: value.album_type,
            duration: value.duration,
            parental_warning: value.parental_warning,
            isrc: value.isrc,
            title: format_title(&value.title, value.version.as_deref()),
            api_source: API_SOURCE.clone(),
        })
    }
}

/// API response type for Qobuz track with simplified metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzTrack {
    /// Track identifier.
    pub id: u64,
    /// Track number on the album.
    pub number: u32,
    /// Artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Album release type.
    pub album_type: QobuzAlbumReleaseType,
    /// Album title.
    pub album: String,
    /// Album identifier.
    pub album_id: String,
    /// Whether track artwork is available.
    pub contains_cover: bool,
    /// Track duration in seconds.
    pub duration: u32,
    /// Whether the track has explicit content.
    pub parental_warning: bool,
    /// International Standard Recording Code.
    pub isrc: String,
    /// Track title.
    pub title: String,
    /// Source API identifier.
    pub api_source: ApiSource,
}

impl From<ApiQobuzTrack> for moosicbox_music_models::api::ApiTrack {
    fn from(value: ApiQobuzTrack) -> Self {
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

/// API response type for Qobuz artist with simplified metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiQobuzArtist {
    /// Artist identifier.
    pub id: u64,
    /// Whether artist photo is available.
    pub contains_cover: bool,
    /// Artist name.
    pub title: String,
    /// Source API identifier.
    pub api_source: ApiSource,
}

static QOBUZ_ACCESS_TOKEN_HEADER: &str = "x-qobuz-access-token";
static QOBUZ_APP_ID_HEADER: &str = "x-qobuz-app-id";
static QOBUZ_APP_SECRET_HEADER: &str = "x-qobuz-app-secret";

/// Query parameters for retrieving detailed artist information from Qobuz.
///
/// Used to fetch artist metadata including name, photo, and other details.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtistQuery {
    /// Qobuz artist identifier to fetch.
    artist_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/artists",
        description = "Get Qobuz artist by ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Qobuz artist ID to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz artist for the specified ID",
                body = ApiArtist,
            )
        )
    )
)]
#[route("/artists", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        &db,
        &query.artist_id.into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.into()))
}

/// Query parameters for retrieving a user's favorited artists from Qobuz.
///
/// Returns a paginated list of artists marked as favorites by the authenticated user.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteArtistsQuery {
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/artists",
        description = "Get Qobuz favorited artists",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Qobuz favorited artists",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteArtistsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
            #[cfg(feature = "db")]
            &db,
            query.offset,
            query.limit,
            req.headers()
                .get(QOBUZ_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            req.headers()
                .get(QOBUZ_APP_ID_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?
        .map(Into::into)
        .into(),
    ))
}

/// Query parameters for retrieving detailed album information from Qobuz.
///
/// Used to fetch album metadata including tracks, artwork, and release details.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbumQuery {
    /// Qobuz album identifier to fetch.
    album_id: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/albums",
        description = "Get Qobuz album by ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Qobuz album ID to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz album for the specified ID",
                body = ApiAlbum,
            )
        )
    )
)]
#[route("/albums", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        &db,
        &query.album_id.clone().into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.try_into().map_err(ErrorInternalServerError)?))
}

/// Album release type categories for API requests.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumReleaseType {
    /// Standard studio album (LP).
    Lp,
    /// Live recording.
    Live,
    /// Compilation album.
    Compilations,
    /// EPs and Singles.
    EpsAndSingles,
    /// Other release type.
    Other,
    /// Download-only release.
    Download,
}

impl From<AlbumReleaseType> for QobuzAlbumReleaseType {
    fn from(value: AlbumReleaseType) -> Self {
        match value {
            AlbumReleaseType::Lp => Self::Album,
            AlbumReleaseType::Live => Self::Live,
            AlbumReleaseType::Compilations => Self::Compilation,
            AlbumReleaseType::EpsAndSingles => Self::EpSingle,
            AlbumReleaseType::Other => Self::Other,
            AlbumReleaseType::Download => Self::Download,
        }
    }
}

/// Sort options for album listings in API requests.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumSort {
    /// Sort by release date.
    ReleaseDate,
    /// Sort by relevance.
    Relevant,
    /// Sort by release date with priority weighting.
    ReleaseDateByPriority,
}

impl From<AlbumSort> for QobuzAlbumSort {
    fn from(value: AlbumSort) -> Self {
        match value {
            AlbumSort::ReleaseDate => Self::ReleaseDate,
            AlbumSort::Relevant => Self::Relevant,
            AlbumSort::ReleaseDateByPriority => Self::ReleaseDateByPriority,
        }
    }
}

/// Sort order direction for album listings in API requests.
#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumOrder {
    /// Ascending order.
    Asc,
    /// Descending order (default).
    #[default]
    Desc,
}

impl From<AlbumOrder> for QobuzAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::Asc => Self::Asc,
            AlbumOrder::Desc => Self::Desc,
        }
    }
}

/// Query parameters for retrieving albums and releases by a specific artist.
///
/// Supports filtering, sorting, and pagination of an artist's discography including
/// studio albums, live recordings, compilations, EPs, and singles.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtistAlbumsQuery {
    /// Artist identifier whose albums to fetch.
    artist_id: u64,
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
    /// Filter by release type (album, live, compilation, etc.).
    release_type: Option<AlbumReleaseType>,
    /// Sort property to order results.
    sort: Option<AlbumSort>,
    /// Sort direction (ascending or descending).
    order: Option<AlbumOrder>,
    /// Number of tracks to return for each album.
    track_size: Option<u8>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/artists/albums",
        description = "Get Qobuz albums for the specified artist ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "Qobuz artist ID to fetch albums for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("releaseType" = Option<AlbumReleaseType>, Query, description = "Release type of album to filter by"),
            ("sort" = Option<AlbumSort>, Query, description = "Sort property to sort the albums by"),
            ("order" = Option<AlbumOrder>, Query, description = "Sort order to order the albums by"),
            ("trackSize" = Option<u8>, Query, description = "The amount of tracks to return for the albums"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz albums for the specified artist",
                body = Value,
            )
        )
    )
)]
#[route("/artists/albums", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiRelease>>> {
    let albums: Page<QobuzRelease> = artist_albums(
        #[cfg(feature = "db")]
        &db,
        &query.artist_id.into(),
        query.offset,
        query.limit,
        query.release_type.map(Into::into),
        query.sort.map(Into::into),
        query.order.map(Into::into),
        query.track_size,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    Ok(Json(albums.into()))
}

/// Query parameters for retrieving a user's favorited albums from Qobuz.
///
/// Returns a paginated list of albums marked as favorites by the authenticated user,
/// optionally filtered by release type.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteAlbumsQuery {
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
    /// Filter by album release type.
    album_type: Option<QobuzAlbumReleaseType>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/albums",
        description = "Get Qobuz favorited albums",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("albumType" = Option<QobuzAlbumReleaseType>, Query, description = "Album type to filter with"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Qobuz favorited albums",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    let albums: Page<QobuzAlbum> = favorite_albums(
        #[cfg(feature = "db")]
        &db,
        query.offset,
        query.limit,
        query.album_type,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    let albums: Page<ApiAlbum> = albums
        .map(TryInto::try_into)
        .transpose()
        .map_err(ErrorInternalServerError)?;

    Ok(Json(albums.into()))
}

/// Query parameters for retrieving the track listing of a specific album.
///
/// Returns paginated track information including titles, artists, durations,
/// and track numbers for the specified album.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbumTracksQuery {
    /// Album identifier whose tracks to fetch.
    album_id: String,
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/albums/tracks",
        description = "Get Qobuz tracks for the specified album ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "Qobuz album ID to fetch tracks for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz tracks for the specified album",
                body = Value,
            )
        )
    )
)]
#[route("/albums/tracks", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<QobuzTrack> = album_tracks(
        #[cfg(feature = "db")]
        &db,
        &query.album_id.clone().into(),
        query.offset,
        query.limit,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    Ok(Json(tracks.into()))
}

/// Query parameters for retrieving detailed track information from Qobuz.
///
/// Used to fetch track metadata including title, artist, album, duration, and audio quality.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrackQuery {
    /// Qobuz track identifier to fetch.
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/tracks",
        description = "Get Qobuz track by ID",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "Qobuz track ID to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz track for the specified ID",
                body = ApiAlbum,
            )
        )
    )
)]
#[route("/tracks", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        &db,
        &query.track_id.into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.into()))
}

/// Query parameters for retrieving a user's favorited tracks from Qobuz.
///
/// Returns a paginated list of tracks marked as favorites by the authenticated user.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzFavoriteTracksQuery {
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/tracks",
        description = "Get Qobuz favorited tracks",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "Page of Qobuz favorited tracks",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteTracksQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<QobuzTrack> = favorite_tracks(
        #[cfg(feature = "db")]
        &db,
        query.offset,
        query.limit,
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?
    .into();

    Ok(Json(tracks.into()))
}

/// Query parameters for obtaining a direct streaming URL for a track.
///
/// Used to retrieve time-limited, signed URLs for playing tracks at the specified
/// audio quality level (e.g., lossy, lossless, hi-res).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrackFileUrlQuery {
    /// Desired audio quality for the stream.
    audio_quality: QobuzAudioQuality,
    /// Track identifier to get streaming URL for.
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/track/url",
        description = "Get Qobuz track file stream URL",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("audioQuality" = QobuzAudioQuality, Query, description = "Audio quality to fetch the file stream for"),
            ("trackId" = u64, Query, description = "Qobuz track ID to fetch track stream URL for"),
        ),
        responses(
            (
                status = 200,
                description = "Qobuz track URL for the specified ID",
                body = ApiAlbum,
            )
        )
    )
)]
#[route("/track/url", method = "GET")]
/// # Panics
///
/// * If the `x-qobuz-access-token`, `x-qobuz-app-id`, or `x-qobuz-app-secret` headers contain invalid UTF-8 bytes
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackFileUrlQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "url": track_file_url(
            #[cfg(feature = "db")]
            &db,
            &query.track_id.into(),
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
    })))
}

/// Query parameters for performing a full-text search across the Qobuz catalog.
///
/// Searches artists, albums, and tracks simultaneously, returning combined paginated
/// results ranked by relevance to the query string.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzSearchQuery {
    /// Search query string to match against artists, albums, and tracks.
    query: String,
    /// Starting offset for pagination.
    offset: Option<u32>,
    /// Maximum number of results to return.
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/search",
        description = "Search the Qobuz library for artists/albums/tracks that fuzzy match the query",
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
/// # Panics
///
/// * If the `x-qobuz-access-token` or `x-qobuz-app-id` headers contain invalid UTF-8 bytes
pub async fn search_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzSearchQuery>,
    #[cfg(feature = "db")] db: LibraryDatabase,
) -> Result<Json<ApiSearchResultsResponse>> {
    let results = search(
        #[cfg(feature = "db")]
        &db,
        &query.query,
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

    Ok(Json(results.into()))
}
