use actix_web::{
    error::{ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized},
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use moosicbox_core::sqlite::models::{yt::YtSearchResults, ToApi};
use moosicbox_paging::Page;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    add_favorite_album, add_favorite_artist, add_favorite_track, album, album_tracks, artist,
    artist_albums, device_authorization, device_authorization_token, favorite_albums,
    favorite_artists, favorite_tracks, remove_favorite_album, remove_favorite_artist,
    remove_favorite_track, search, track, track_file_url, track_playback_info,
    AuthenticatedRequestError, SearchType, YtAddFavoriteAlbumError, YtAddFavoriteArtistError,
    YtAddFavoriteTrackError, YtAlbum, YtAlbumError, YtAlbumOrder, YtAlbumOrderDirection,
    YtAlbumTracksError, YtAlbumType, YtArtist, YtArtistAlbumsError, YtArtistError, YtArtistOrder,
    YtArtistOrderDirection, YtAudioQuality, YtDeviceAuthorizationError,
    YtDeviceAuthorizationTokenError, YtDeviceType, YtFavoriteAlbumsError, YtFavoriteArtistsError,
    YtFavoriteTracksError, YtRemoveFavoriteAlbumError, YtRemoveFavoriteArtistError,
    YtRemoveFavoriteTrackError, YtSearchError, YtTrack, YtTrackError, YtTrackFileUrlError,
    YtTrackOrder, YtTrackOrderDirection, YtTrackPlaybackInfo, YtTrackPlaybackInfoError,
};

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
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtAlbum {
    pub id: String,
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
    Yt(ApiYtTrack),
}

impl ToApi<ApiTrack> for YtTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Yt(ApiYtTrack {
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
pub struct ApiYtTrack {
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
    Yt(ApiYtArtist),
}

impl ToApi<ApiArtist> for YtArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Yt(ApiYtArtist {
            id: self.id,
            contains_cover: self.contains_cover,
            popularity: self.popularity,
            title: self.name.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiYtArtist {
    pub id: u64,
    pub contains_cover: bool,
    pub popularity: u32,
    pub title: String,
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

#[route("/yt/auth/device-authorization", method = "POST")]
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

#[route("/yt/auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    query: web::Query<YtDeviceAuthorizationTokenQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization_token(
            #[cfg(feature = "db")]
            &**data.database,
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

#[route("/yt/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackFileUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            #[cfg(feature = "db")]
            &**data.database,
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

#[route("/yt/track/playback-info", method = "GET")]
pub async fn track_playback_info_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackPlaybackInfoQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<YtTrackPlaybackInfo>> {
    Ok(Json(
        track_playback_info(
            #[cfg(feature = "db")]
            &**data.database,
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

#[route("/yt/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
            #[cfg(feature = "db")]
            data.database.clone(),
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

#[route("/yt/favorites/artists", method = "POST")]
pub async fn add_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_artist(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/artists", method = "DELETE")]
pub async fn remove_favorite_artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteArtistsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_artist(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        favorite_albums(
            #[cfg(feature = "db")]
            data.database.clone(),
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

#[route("/yt/favorites/albums", method = "POST")]
pub async fn add_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_album(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/albums", method = "DELETE")]
pub async fn remove_favorite_album_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_album(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/tracks", method = "POST")]
pub async fn add_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<YtAddFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_track(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/tracks", method = "DELETE")]
pub async fn remove_favorite_track_endpoint(
    req: HttpRequest,
    query: web::Query<YtRemoveFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_track(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<YtFavoriteTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        favorite_tracks(
            #[cfg(feature = "db")]
            data.database.clone(),
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

#[route("/yt/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<YtArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        artist_albums(
            #[cfg(feature = "db")]
            data.database.clone(),
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

#[route("/yt/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    req: HttpRequest,
    query: web::Query<YtAlbumTracksQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        album_tracks(
            #[cfg(feature = "db")]
            data.database.clone(),
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

#[route("/yt/albums", method = "GET")]
pub async fn album_endpoint(
    req: HttpRequest,
    query: web::Query<YtAlbumQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAlbum>> {
    let album = album(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<YtArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiArtist>> {
    let artist = artist(
        #[cfg(feature = "db")]
        &**data.database,
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

#[route("/yt/tracks", method = "GET")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<YtTrackQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTrack>> {
    let track = track(
        #[cfg(feature = "db")]
        &**data.database,
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
    include_contributions: Option<bool>,
    include_did_you_mean: Option<bool>,
    include_user_playlists: Option<bool>,
    supports_user_data: Option<bool>,
    types: Option<Vec<SearchType>>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
}

#[route("/yt/search", method = "GET")]
pub async fn search_endpoint(
    req: HttpRequest,
    query: web::Query<YtSearchQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<YtSearchResults>> {
    Ok(Json(
        search(
            #[cfg(feature = "db")]
            &**data.database,
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
                .map(|x| x.into_iter().map(|x| x.into()).collect::<Vec<_>>()),
            query.country_code.clone(),
            query.locale.clone(),
            query.device_type,
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
        )
        .await?,
    ))
}
