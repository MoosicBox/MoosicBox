use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized},
    route,
    web::{self, Json},
    HttpRequest, Result, Scope,
};
use moosicbox_core::sqlite::models::{ApiSource, ApiSources, ToApi, TrackApiSource};
#[cfg(feature = "db")]
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_paging::Page;
use moosicbox_search::models::ApiSearchResultsResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    add_favorite_album, add_favorite_artist, add_favorite_track, album, album_tracks, artist,
    artist_albums, device_authorization, device_authorization_token, favorite_albums,
    favorite_artists, favorite_tracks, remove_favorite_album, remove_favorite_artist,
    remove_favorite_track, search, track, track_file_url, track_playback_info,
    AuthenticatedRequestError, YtAddFavoriteAlbumError, YtAddFavoriteArtistError,
    YtAddFavoriteTrackError, YtAlbum, YtAlbumError, YtAlbumOrder, YtAlbumOrderDirection,
    YtAlbumTracksError, YtAlbumType, YtArtist, YtArtistAlbumsError, YtArtistError, YtArtistOrder,
    YtArtistOrderDirection, YtAudioQuality, YtDeviceAuthorizationError,
    YtDeviceAuthorizationTokenError, YtDeviceType, YtFavoriteAlbumsError, YtFavoriteArtistsError,
    YtFavoriteTracksError, YtRemoveFavoriteAlbumError, YtRemoveFavoriteArtistError,
    YtRemoveFavoriteTrackError, YtSearchError, YtTrack, YtTrackError, YtTrackFileUrlError,
    YtTrackOrder, YtTrackOrderDirection, YtTrackPlaybackInfo, YtTrackPlaybackInfoError,
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
pub struct Api;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Yt(ApiYtAlbum),
}

impl ToApi<ApiAlbum> for YtAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Yt(ApiYtAlbum {
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
            api_source: ApiSource::Yt,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: String,
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
    Yt(ApiYtTrack),
}

impl ToApi<ApiTrack> for YtTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Yt(ApiYtTrack {
            id: self.id,
            number: self.track_number,
            album: self.album.clone(),
            album_id: self.album_id,
            album_type: self.album_type,
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
            api_source: ApiSource::Yt,
        })
    }
}

impl From<ApiTrack> for moosicbox_core::sqlite::models::ApiTrack {
    fn from(value: ApiTrack) -> Self {
        let ApiTrack::Yt(track) = value;
        track.into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtTrack {
    pub id: String,
    pub number: u32,
    pub album: String,
    pub album_id: String,
    pub album_type: YtAlbumType,
    pub artist: String,
    pub artist_id: String,
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

impl From<ApiYtTrack> for moosicbox_core::sqlite::models::ApiTrack {
    fn from(value: ApiYtTrack) -> Self {
        Self {
            track_id: value.id.clone().into(),
            number: value.number,
            title: value.title,
            duration: value.duration as f64,
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
            track_source: TrackApiSource::Yt,
            api_source: ApiSource::Yt,
            sources: ApiSources::default().with_source(ApiSource::Yt, value.id.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Yt(ApiYtArtist),
}

impl ToApi<ApiArtist> for YtArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Yt(ApiYtArtist {
            id: self.id,
            contains_cover: self.contains_cover,
            popularity: self.popularity,
            title: self.name.clone(),
            api_source: ApiSource::Yt,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtArtist {
    pub id: String,
    pub contains_cover: bool,
    pub popularity: u32,
    pub title: String,
    pub api_source: ApiSource,
}

static TIDAL_ACCESS_TOKEN_HEADER: &str = "x-yt-access-token";

impl From<YtDeviceAuthorizationError> for actix_web::Error {
    fn from(err: YtDeviceAuthorizationError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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

impl From<YtDeviceAuthorizationTokenError> for actix_web::Error {
    fn from(err: YtDeviceAuthorizationTokenError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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

impl From<YtTrackFileUrlError> for actix_web::Error {
    fn from(e: YtTrackFileUrlError) -> Self {
        match e {
            YtTrackFileUrlError::AuthenticatedRequest(e) => ErrorUnauthorized(e.to_string()),
            YtTrackFileUrlError::Parse(_) => ErrorInternalServerError(e.to_string()),
        }
    }
}

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
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    })))
}

impl From<YtTrackPlaybackInfoError> for actix_web::Error {
    fn from(err: YtTrackPlaybackInfoError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}

impl From<YtFavoriteArtistsError> for actix_web::Error {
    fn from(err: YtFavoriteArtistsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            query.user_id,
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<YtAddFavoriteArtistError> for actix_web::Error {
    fn from(err: YtAddFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtRemoveFavoriteArtistError> for actix_web::Error {
    fn from(err: YtRemoveFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtFavoriteAlbumsError> for actix_web::Error {
    fn from(err: YtFavoriteAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            query.user_id,
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<YtAddFavoriteAlbumError> for actix_web::Error {
    fn from(err: YtAddFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtRemoveFavoriteAlbumError> for actix_web::Error {
    fn from(err: YtRemoveFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtAddFavoriteTrackError> for actix_web::Error {
    fn from(err: YtAddFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtRemoveFavoriteTrackError> for actix_web::Error {
    fn from(err: YtRemoveFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
        query.user_id,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<YtFavoriteTracksError> for actix_web::Error {
    fn from(err: YtFavoriteTracksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
    Ok(Json(
        favorite_tracks(
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
        .to_api()
        .into(),
    ))
}

impl From<YtArtistAlbumsError> for actix_web::Error {
    fn from(err: YtArtistAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<AlbumType> for YtAlbumType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => YtAlbumType::Lp,
            AlbumType::EpsAndSingles => YtAlbumType::EpsAndSingles,
            AlbumType::Compilations => YtAlbumType::Compilations,
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

impl From<YtAlbumTracksError> for actix_web::Error {
    fn from(err: YtAlbumTracksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
    Ok(Json(
        album_tracks(
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
        .to_api()
        .into(),
    ))
}

impl From<YtAlbumError> for actix_web::Error {
    fn from(err: YtAlbumError) -> Self {
        log::error!("{err:?}");
        if let YtAlbumError::AuthenticatedRequest(AuthenticatedRequestError::RequestFailed(
            status,
            _,
        )) = err
        {
            if status == 404 {
                return ErrorNotFound("Yt album not found");
            }
        }

        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(album.to_api()))
}

impl From<YtArtistError> for actix_web::Error {
    fn from(err: YtArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(artist.to_api()))
}

impl From<YtTrackError> for actix_web::Error {
    fn from(err: YtTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

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
            .get(TIDAL_ACCESS_TOKEN_HEADER)
            .map(|x| x.to_str().unwrap().to_string()),
    )
    .await?;

    Ok(Json(track.to_api()))
}

impl From<YtSearchError> for actix_web::Error {
    fn from(err: YtSearchError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["YouTube Music"],
        get,
        path = "/search",
        description = "Search the YouTube Music library for artists/albums/tracks that fuzzy match the query",
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
    query: web::Query<YtSearchQuery>,
) -> Result<Json<ApiSearchResultsResponse>> {
    Ok(Json(
        search(&query.query, query.offset, query.limit)
            .await?
            .into(),
    ))
}
