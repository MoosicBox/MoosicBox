use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::{integer_range::parse_integer_ranges_to_ids, sqlite::models::ToApi};
use moosicbox_paging::Page;
use moosicbox_search::models::ApiSearchResultsResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

use crate::{
    add_favorite_album, add_favorite_artist, add_favorite_track, album, album_tracks, artist,
    artist_albums, favorite_artists, favorite_tracks, reindex_global_search_index,
    remove_favorite_album, remove_favorite_artist, remove_favorite_track, search, track,
    track_file_url, LibraryAddFavoriteAlbumError, LibraryAddFavoriteArtistError,
    LibraryAddFavoriteTrackError, LibraryAlbum, LibraryAlbumError, LibraryAlbumTracksError,
    LibraryAlbumType, LibraryArtist, LibraryArtistAlbumsError, LibraryArtistError,
    LibraryArtistOrder, LibraryArtistOrderDirection, LibraryAudioQuality,
    LibraryFavoriteArtistsError, LibraryFavoriteTracksError, LibraryRemoveFavoriteAlbumError,
    LibraryRemoveFavoriteArtistError, LibraryRemoveFavoriteTrackError, LibrarySearchError,
    LibraryTrack, LibraryTrackError, LibraryTrackFileUrlError, LibraryTrackOrder,
    LibraryTrackOrderDirection, ReindexError, SearchType,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Library(ApiLibraryAlbum),
}

impl ToApi<ApiAlbum> for LibraryAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Library(ApiLibraryAlbum {
            id: self.id as u64,
            artist: self.artist,
            artist_id: self.artist_id as u64,
            contains_cover: self.artwork.is_some(),
            explicit: false,
            date_released: self.date_released,
            title: self.title,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryAlbum {
    pub id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub explicit: bool,
    pub date_released: Option<String>,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    Library(ApiLibraryTrack),
}

impl ToApi<ApiTrack> for LibraryTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Library(ApiLibraryTrack {
            id: self.id as u64,
            number: self.number as u32,
            album: self.album,
            album_id: self.album_id as u64,
            artist: self.artist,
            artist_id: self.artist_id as u64,
            contains_cover: self.artwork.is_some(),
            duration: self.duration,
            explicit: false,
            title: self.title,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryTrack {
    pub id: u64,
    pub number: u32,
    pub album: String,
    pub album_id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub duration: f64,
    pub explicit: bool,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Library(ApiLibraryArtist),
}

impl ToApi<ApiArtist> for LibraryArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Library(ApiLibraryArtist {
            id: self.id as u64,
            contains_cover: self.cover.is_some(),
            title: self.title,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryArtist {
    pub id: u64,
    pub contains_cover: bool,
    pub title: String,
}

impl From<LibraryTrackFileUrlError> for actix_web::Error {
    fn from(e: LibraryTrackFileUrlError) -> Self {
        match e {
            LibraryTrackFileUrlError::NoFile => ErrorNotFound("Track file not found"),
            LibraryTrackFileUrlError::LibraryTrack(_) => ErrorInternalServerError(e.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrackFileUrlQuery {
    audio_quality: LibraryAudioQuality,
    track_id: u64,
}

#[route("/library/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    query: web::Query<LibraryTrackFileUrlQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            &**data.database,
            query.audio_quality,
            &query.track_id.into(),

        )
        .await?,
    })))
}

impl From<LibraryFavoriteArtistsError> for actix_web::Error {
    fn from(err: LibraryFavoriteArtistsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFavoriteArtistsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<LibraryArtistOrder>,
    order_direction: Option<LibraryArtistOrderDirection>,
}

#[route("/library/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    query: web::Query<LibraryFavoriteArtistsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiArtist>>> {
    Ok(Json(
        favorite_artists(
            data.database.clone(),
            query.offset,
            query.limit,
            query.order,
            query.order_direction,
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<LibraryAddFavoriteArtistError> for actix_web::Error {
    fn from(err: LibraryAddFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAddFavoriteArtistsQuery {
    artist_id: u64,
}

#[route("/library/favorites/artists", method = "POST")]
pub async fn add_favorite_artist_endpoint(
    query: web::Query<LibraryAddFavoriteArtistsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_artist(&**data.database, &query.artist_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<LibraryRemoveFavoriteArtistError> for actix_web::Error {
    fn from(err: LibraryRemoveFavoriteArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRemoveFavoriteArtistsQuery {
    artist_id: u64,
}

#[route("/library/favorites/artists", method = "DELETE")]
pub async fn remove_favorite_artist_endpoint(
    query: web::Query<LibraryRemoveFavoriteArtistsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_artist(&**data.database, &query.artist_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

// impl From<LibraryFavoriteAlbumsError> for actix_web::Error {
//     fn from(err: LibraryFavoriteAlbumsError) -> Self {
//         log::error!("{err:?}");
//         ErrorInternalServerError(err.to_string())
//     }
// }

// #[derive(Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct LibraryFavoriteAlbumsQuery {
//     offset: Option<u32>,
//     limit: Option<u32>,
//     order: Option<LibraryAlbumOrder>,
//     order_direction: Option<LibraryAlbumOrderDirection>,
// }

// #[route("/library/favorites/albums", method = "GET")]
// pub async fn favorite_albums_endpoint(
//     query: web::Query<LibraryFavoriteAlbumsQuery>,
//     data: web::Data<moosicbox_core::app::AppState>,
// ) -> Result<Json<Page<ApiAlbum>>> {
//     Ok(Json(
//         favorite_albums(
//             data.database.clone(),
//             query.offset,
//             query.limit,
//             query.order,
//             query.order_direction,
//         )
//         .await?
//         .to_api()
//         .into(),
//     ))
// }

impl From<LibraryAddFavoriteAlbumError> for actix_web::Error {
    fn from(err: LibraryAddFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAddFavoriteAlbumsQuery {
    album_id: u64,
}

#[route("/library/favorites/albums", method = "POST")]
pub async fn add_favorite_album_endpoint(
    query: web::Query<LibraryAddFavoriteAlbumsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_album(&**data.database, &query.album_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<LibraryRemoveFavoriteAlbumError> for actix_web::Error {
    fn from(err: LibraryRemoveFavoriteAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRemoveFavoriteAlbumsQuery {
    album_id: u64,
}

#[route("/library/favorites/albums", method = "DELETE")]
pub async fn remove_favorite_album_endpoint(
    query: web::Query<LibraryRemoveFavoriteAlbumsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_album(&**data.database, &query.album_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<LibraryAddFavoriteTrackError> for actix_web::Error {
    fn from(err: LibraryAddFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAddFavoriteTracksQuery {
    track_id: u64,
}

#[route("/library/favorites/tracks", method = "POST")]
pub async fn add_favorite_track_endpoint(
    query: web::Query<LibraryAddFavoriteTracksQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    add_favorite_track(&**data.database, &query.track_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<LibraryRemoveFavoriteTrackError> for actix_web::Error {
    fn from(err: LibraryRemoveFavoriteTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRemoveFavoriteTracksQuery {
    track_id: u64,
}

#[route("/library/favorites/tracks", method = "DELETE")]
pub async fn remove_favorite_track_endpoint(
    query: web::Query<LibraryRemoveFavoriteTracksQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    remove_favorite_track(&**data.database, &query.track_id.into()).await?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

impl From<LibraryFavoriteTracksError> for actix_web::Error {
    fn from(err: LibraryFavoriteTracksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFavoriteTracksQuery {
    track_ids: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<LibraryTrackOrder>,
    order_direction: Option<LibraryTrackOrderDirection>,
}

#[route("/library/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    query: web::Query<LibraryFavoriteTracksQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    let track_ids = query
        .track_ids
        .as_ref()
        .map(|ids| parse_integer_ranges_to_ids(ids.as_str()))
        .transpose()
        .map_err(|e| ErrorBadRequest(format!("Invalid track id values: {e:?}")))?;

    Ok(Json(
        favorite_tracks(
            data.database.clone(),
            track_ids.as_deref(),
            query.offset,
            query.limit,
            query.order,
            query.order_direction,
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<LibraryArtistAlbumsError> for actix_web::Error {
    fn from(err: LibraryArtistAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryArtistAlbumsQuery {
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<AlbumType>,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumType {
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<AlbumType> for LibraryAlbumType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => LibraryAlbumType::Lp,
            AlbumType::EpsAndSingles => LibraryAlbumType::EpsAndSingles,
            AlbumType::Compilations => LibraryAlbumType::Compilations,
        }
    }
}

#[route("/library/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    query: web::Query<LibraryArtistAlbumsQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        artist_albums(
            data.database.clone(),
            &query.artist_id.into(),
            query.offset,
            query.limit,
            query.album_type.map(|t| t.into()),
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<LibraryAlbumTracksError> for actix_web::Error {
    fn from(err: LibraryAlbumTracksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAlbumTracksQuery {
    album_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/library/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    query: web::Query<LibraryAlbumTracksQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiTrack>>> {
    Ok(Json(
        album_tracks(
            data.database.clone(),
            &query.album_id.into(),
            query.offset,
            query.limit,
        )
        .await?
        .to_api()
        .into(),
    ))
}

impl From<LibraryAlbumError> for actix_web::Error {
    fn from(err: LibraryAlbumError) -> Self {
        log::error!("{err:?}");
        match err {
            LibraryAlbumError::NotFound => ErrorNotFound("Library album not found"),
            LibraryAlbumError::Db(_) => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAlbumQuery {
    album_id: u64,
}

#[route("/library/albums", method = "GET")]
pub async fn album_endpoint(
    query: web::Query<LibraryAlbumQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAlbum>> {
    let album = album(&**data.database, &query.album_id.into()).await?;

    Ok(Json(album.to_api()))
}

impl From<LibraryArtistError> for actix_web::Error {
    fn from(err: LibraryArtistError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryArtistQuery {
    artist_id: u64,
}

#[route("/library/artists", method = "GET")]
pub async fn artist_endpoint(
    query: web::Query<LibraryArtistQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiArtist>> {
    let artist = artist(&**data.database, &query.artist_id.into()).await?;

    Ok(Json(artist.to_api()))
}

impl From<LibraryTrackError> for actix_web::Error {
    fn from(err: LibraryTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrackQuery {
    track_id: u64,
}

#[route("/library/tracks", method = "GET")]
pub async fn track_endpoint(
    query: web::Query<LibraryTrackQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiTrack>> {
    let track = track(&**data.database, &query.track_id.into()).await?;

    Ok(Json(track.to_api()))
}

impl From<LibrarySearchError> for actix_web::Error {
    fn from(err: LibrarySearchError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
    types: Option<Vec<SearchType>>,
}

#[route("/library/search", method = "GET")]
pub async fn search_endpoint(
    query: web::Query<LibrarySearchQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiSearchResultsResponse>> {
    let results = search(
        &**data.database,
        &query.query,
        query.offset,
        query.limit,
        query
            .types
            .clone()
            .map(|x| x.into_iter().map(|x| x.into()).collect::<Vec<_>>()),
    )
    .await?;

    Ok(Json(results.into()))
}

impl From<ReindexError> for actix_web::Error {
    fn from(err: ReindexError) -> Self {
        log::error!("{err:?}");
        match err {
            ReindexError::Db(_)
            | ReindexError::RecreateIndex(_)
            | ReindexError::PopulateIndex(_) => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReindexQuery {}

#[route("/library/reindex", method = "POST")]
pub async fn reindex_endpoint(
    _query: web::Query<ReindexQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    reindex_global_search_index(&**data.database).await?;

    Ok(Json(serde_json::json!({"success": true})))
}
