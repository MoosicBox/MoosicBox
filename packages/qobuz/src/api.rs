use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorUnauthorized},
    route,
    web::{self, Json},
    HttpRequest, Result, Scope,
};
use moosicbox_core::sqlite::models::ToApi;
use moosicbox_paging::Page;
use moosicbox_search::models::ApiSearchResultsResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    album, album_tracks, artist, artist_albums, favorite_albums, favorite_artists, favorite_tracks,
    format_title, search, track, track_file_url, user_login, QobuzAlbum, QobuzAlbumError,
    QobuzAlbumOrder, QobuzAlbumReleaseType, QobuzAlbumSort, QobuzAlbumTracksError, QobuzArtist,
    QobuzArtistAlbumsError, QobuzArtistError, QobuzAudioQuality, QobuzFavoriteAlbumsError,
    QobuzFavoriteArtistsError, QobuzFavoriteTracksError, QobuzRelease, QobuzSearchError,
    QobuzTrack, QobuzTrackError, QobuzTrackFileUrlError, QobuzUserLoginError,
};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Qobuz(ApiQobuzAlbum),
}

impl ToApi<ApiAlbum> for QobuzAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Qobuz(ApiQobuzAlbum {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.cover_url().is_some(),
            duration: self.duration,
            title: format_title(&self.title, self.version.as_deref()),
            parental_warning: self.parental_warning,
            number_of_tracks: self.tracks_count,
            date_released: self.release_date_original.clone(),
        })
    }
}

impl From<QobuzUserLoginError> for actix_web::Error {
    fn from(err: QobuzUserLoginError) -> Self {
        match err {
            QobuzUserLoginError::Unauthorized => ErrorUnauthorized(err.to_string()),
            QobuzUserLoginError::Reqwest(_)
            | QobuzUserLoginError::NoAccessTokenAvailable
            | QobuzUserLoginError::NoAppIdAvailable
            | QobuzUserLoginError::Parse(_)
            | QobuzUserLoginError::QobuzFetchLoginSource(_)
            | QobuzUserLoginError::QobuzFetchBundleSource(_)
            | QobuzUserLoginError::QobuzFetchAppSecrets(_)
            | QobuzUserLoginError::FailedToFetchAppId => ErrorInternalServerError(err.to_string()),
            #[cfg(feature = "db")]
            QobuzUserLoginError::Database(_) | QobuzUserLoginError::Db(_) => {
                ErrorInternalServerError(err.to_string())
            }
        }
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        post,
        path = "/auth/login",
        description = "Login to Qobuz",
        params(
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
pub async fn user_login_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzUserLoginQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        user_login(
            #[cfg(feature = "db")]
            data.database.clone(),
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
    pub contains_cover: bool,
    pub duration: u32,
    pub parental_warning: bool,
    pub number_of_tracks: u32,
    pub date_released: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzRelease {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub duration: u32,
    pub parental_warning: bool,
    pub number_of_tracks: u32,
    pub date_released: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiRelease {
    Qobuz(ApiQobuzRelease),
}

impl ToApi<ApiRelease> for QobuzRelease {
    fn to_api(self) -> ApiRelease {
        ApiRelease::Qobuz(ApiQobuzRelease {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.cover_url().is_some(),
            duration: self.duration,
            title: format_title(&self.title, self.version.as_deref()),
            parental_warning: self.parental_warning,
            number_of_tracks: self.tracks_count,
            date_released: self.release_date_original.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    Qobuz(ApiQobuzTrack),
}

impl ToApi<ApiTrack> for QobuzTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Qobuz(ApiQobuzTrack {
            id: self.id,
            number: self.track_number,
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            album: self.album.clone(),
            album_id: self.album_id.clone(),
            contains_cover: self.cover_url().is_some(),
            duration: self.duration,
            parental_warning: self.parental_warning,
            isrc: self.isrc.clone(),
            title: format_title(&self.title, self.version.as_deref()),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzTrack {
    pub id: u64,
    pub number: u32,
    pub artist: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: String,
    pub contains_cover: bool,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Qobuz(ApiQobuzArtist),
}

impl ToApi<ApiArtist> for QobuzArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Qobuz(ApiQobuzArtist {
            id: self.id,
            contains_cover: self.cover_url().is_some(),
            title: self.name.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiQobuzArtist {
    pub id: u64,
    pub contains_cover: bool,
    pub title: String,
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/artists",
        description = "Get Qobuz artist by ID",
        params(
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
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        &**data.database,
        &query.artist_id.into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.to_api()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/artists",
        description = "Get Qobuz favorited artists",
        params(
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
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
            #[cfg(feature = "db")]
            data.database.clone(),
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
        .to_api()
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/albums",
        description = "Get Qobuz album by ID",
        params(
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
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        &**data.database,
        &query.album_id.clone().into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.to_api()))
}

impl From<QobuzArtistAlbumsError> for actix_web::Error {
    fn from(err: QobuzArtistAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumReleaseType {
    All,
    Lp,
    Live,
    Compilations,
    EpsAndSingles,
    Other,
    Download,
}

impl From<AlbumReleaseType> for QobuzAlbumReleaseType {
    fn from(value: AlbumReleaseType) -> Self {
        match value {
            AlbumReleaseType::All => QobuzAlbumReleaseType::All,
            AlbumReleaseType::Lp => QobuzAlbumReleaseType::Album,
            AlbumReleaseType::Live => QobuzAlbumReleaseType::Live,
            AlbumReleaseType::Compilations => QobuzAlbumReleaseType::Compilation,
            AlbumReleaseType::EpsAndSingles => QobuzAlbumReleaseType::EpSingle,
            AlbumReleaseType::Other => QobuzAlbumReleaseType::Other,
            AlbumReleaseType::Download => QobuzAlbumReleaseType::Download,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumSort {
    ReleaseDate,
    Relevant,
    ReleaseDateByPriority,
}

impl From<AlbumSort> for QobuzAlbumSort {
    fn from(value: AlbumSort) -> Self {
        match value {
            AlbumSort::ReleaseDate => QobuzAlbumSort::ReleaseDate,
            AlbumSort::Relevant => QobuzAlbumSort::Relevant,
            AlbumSort::ReleaseDateByPriority => QobuzAlbumSort::ReleaseDateByPriority,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumOrder {
    Asc,
    #[default]
    Desc,
}

impl From<AlbumOrder> for QobuzAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::Asc => QobuzAlbumOrder::Asc,
            AlbumOrder::Desc => QobuzAlbumOrder::Desc,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtistAlbumsQuery {
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    release_type: Option<AlbumReleaseType>,
    sort: Option<AlbumSort>,
    order: Option<AlbumOrder>,
    track_size: Option<u8>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/artists/albums",
        description = "Get Qobuz albums for the specified artist ID",
        params(
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
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiRelease>>> {
    Ok(Json(
        artist_albums(
            #[cfg(feature = "db")]
            data.database.clone(),
            &query.artist_id.into(),
            query.offset,
            query.limit,
            query.release_type.map(|x| x.into()),
            query.sort.map(|x| x.into()),
            query.order.map(|x| x.into()),
            query.track_size,
            req.headers()
                .get(QOBUZ_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            req.headers()
                .get(QOBUZ_APP_ID_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?
        .to_api()
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/albums",
        description = "Get Qobuz favorited albums",
        params(
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
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
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        favorite_albums(
            #[cfg(feature = "db")]
            data.database.clone(),
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
        .to_api()
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/albums/tracks",
        description = "Get Qobuz tracks for the specified album ID",
        params(
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
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzAlbumTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        album_tracks(
            #[cfg(feature = "db")]
            data.database.clone(),
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
        .to_api()
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/tracks",
        description = "Get Qobuz track by ID",
        params(
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
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        &**data.database,
        &query.track_id.into(),
        req.headers()
            .get(QOBUZ_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        req.headers()
            .get(QOBUZ_APP_ID_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.to_api()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/favorites/tracks",
        description = "Get Qobuz favorited tracks",
        params(
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
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        favorite_tracks(
            #[cfg(feature = "db")]
            data.database.clone(),
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
        .to_api()
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/track/url",
        description = "Get Qobuz track file stream URL",
        params(
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
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackFileUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "url": track_file_url(
            #[cfg(feature = "db")]
            &**data.database,
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

impl From<QobuzSearchError> for actix_web::Error {
    fn from(err: QobuzSearchError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzSearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Qobuz"],
        get,
        path = "/search",
        description = "Search the Qobuz library for artists/albums/tracks that fuzzy match the query",
        params(
            ("query" = String, Query, description = "The search query"),
            ("offset" = Option<usize>, Query, description = "Page offset"),
            ("limit" = Option<usize>, Query, description = "Page limit"),
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
    req: HttpRequest,
    query: web::Query<QobuzSearchQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiSearchResultsResponse>> {
    let results = search(
        #[cfg(feature = "db")]
        &**data.database,
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
