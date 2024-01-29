use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use moosicbox_core::sqlite::models::ToApi;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    album, album_tracks, artist, artist_albums, favorite_albums, favorite_artists, favorite_tracks,
    track, track_file_url, user_login, QobuzAlbum, QobuzAlbumError, QobuzAlbumOrder,
    QobuzAlbumReleaseType, QobuzAlbumSort, QobuzAlbumTracksError, QobuzArtist,
    QobuzArtistAlbumsError, QobuzArtistError, QobuzAudioQuality, QobuzFavoriteAlbumsError,
    QobuzFavoriteArtistsError, QobuzFavoriteTracksError, QobuzRelease, QobuzTrack, QobuzTrackError,
    QobuzTrackFileUrlError, QobuzUserLoginError,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Qobuz(ApiQobuzAlbum),
}

impl ToApi<ApiAlbum> for QobuzAlbum {
    fn to_api(&self) -> ApiAlbum {
        ApiAlbum::Qobuz(ApiQobuzAlbum {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.cover_url().is_some(),
            duration: self.duration,
            title: self.title.clone(),
            parental_warning: self.parental_warning,
            number_of_tracks: self.tracks_count,
            date_released: self.release_date_original.clone(),
        })
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
    fn to_api(&self) -> ApiRelease {
        ApiRelease::Qobuz(ApiQobuzRelease {
            id: self.id.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.cover_url().is_some(),
            duration: self.duration,
            title: self.title.clone(),
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
    fn to_api(&self) -> ApiTrack {
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
            title: self.title.clone(),
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
    fn to_api(&self) -> ApiArtist {
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

#[route("/qobuz/artists", method = "GET")]
pub async fn artist_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiArtist>> {
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
) -> Result<Json<ApiAlbum>> {
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
    artist_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
    release_type: Option<AlbumReleaseType>,
    sort: Option<AlbumSort>,
    order: Option<AlbumOrder>,
    track_size: Option<u8>,
}

#[route("/qobuz/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzArtistAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let (items, has_more) = artist_albums(
        #[cfg(feature = "db")]
        data.db.as_ref().expect("Db not set"),
        &query.artist_id,
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
    .await?;

    Ok(Json(serde_json::json!({
        "hasMore": has_more,
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

#[route("/qobuz/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    req: HttpRequest,
    query: web::Query<QobuzTrackFileUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "url": track_file_url(
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
    })))
}
