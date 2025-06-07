#![allow(clippy::module_name_repetitions)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
};
use moosicbox_music_api::models::AlbumsRequest;
use moosicbox_music_api::models::search::api::ApiSearchResultsResponse;
use moosicbox_music_models::{AlbumSort, api::ApiAlbum, id::parse_integer_ranges_to_ids};
use moosicbox_paging::{Page, PagingRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use switchy_database::profiles::LibraryDatabase;

use crate::{
    LibraryAddFavoriteAlbumError, LibraryAddFavoriteArtistError, LibraryAddFavoriteTrackError,
    LibraryAlbumError, LibraryAlbumOrder, LibraryAlbumOrderDirection, LibraryAlbumTracksError,
    LibraryAlbumType, LibraryArtist, LibraryArtistAlbumsError, LibraryArtistError,
    LibraryArtistOrder, LibraryArtistOrderDirection, LibraryAudioQuality,
    LibraryFavoriteAlbumsError, LibraryFavoriteArtistsError, LibraryFavoriteTracksError,
    LibraryRemoveFavoriteAlbumError, LibraryRemoveFavoriteArtistError,
    LibraryRemoveFavoriteTrackError, LibraryTrack, LibraryTrackError, LibraryTrackFileUrlError,
    LibraryTrackOrder, LibraryTrackOrderDirection, ReindexError, SearchType, add_favorite_album,
    add_favorite_artist, add_favorite_track, album, album_tracks, artist, artist_albums,
    favorite_albums, favorite_artists, favorite_tracks, reindex_global_search_index,
    remove_favorite_album, remove_favorite_artist, remove_favorite_track, search, track,
    track_file_url,
};

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(track_file_url_endpoint)
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
        .service(reindex_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Library")),
    paths(
        track_file_url_endpoint,
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
        reindex_endpoint,
    ),
    components(schemas(
        LibraryTrackQuery,
        AlbumType,
        ApiArtist,
        ApiAlbum,
        ApiTrack,
        ApiLibraryArtist,
        ApiLibraryAlbum,
        ApiLibraryTrack,
        ApiSearchResultsResponse,
        moosicbox_music_api::models::search::api::ApiGlobalSearchResult,
        moosicbox_music_api::models::search::api::ApiGlobalArtistSearchResult,
        moosicbox_music_api::models::search::api::ApiGlobalAlbumSearchResult,
        moosicbox_music_api::models::search::api::ApiGlobalTrackSearchResult,
        LibraryArtistOrder,
        LibraryArtistOrderDirection,
        LibraryAlbumOrder,
        LibraryAlbumOrderDirection,
        LibraryTrackOrder,
        LibraryTrackOrderDirection,
        SearchType,
        LibraryAudioQuality,
    ))
)]
pub struct Api;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiTrack {
    Library(ApiLibraryTrack),
}

impl From<LibraryTrack> for ApiTrack {
    fn from(value: LibraryTrack) -> Self {
        Self::Library(ApiLibraryTrack {
            id: value.id,
            number: value.number,
            album: value.album,
            album_id: value.album_id,
            artist: value.artist,
            artist_id: value.artist_id,
            contains_cover: value.artwork.is_some(),
            duration: value.duration,
            explicit: false,
            title: value.title,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiArtist {
    Library(ApiLibraryArtist),
}

impl From<LibraryArtist> for ApiArtist {
    fn from(value: LibraryArtist) -> Self {
        Self::Library(ApiLibraryArtist {
            id: value.id,
            contains_cover: value.cover.is_some(),
            title: value.title,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    track_id: u64,
    audio_quality: LibraryAudioQuality,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/track/url",
        description = "Get track stream URL for the audio",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "The track ID"),
            ("audioQuality" = LibraryAudioQuality, Query, description = "Page offset"),
        ),
        responses(
            (
                status = 200,
                description = "The track URL",
                body = Value,
            )
        )
    )
)]
#[route("/track/url", method = "GET")]
pub async fn track_file_url_endpoint(
    query: web::Query<LibraryTrackFileUrlQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    Ok(Json(serde_json::json!({
        "urls": track_file_url(
            &db,
            query.audio_quality,
            &query.track_id.into(),

        )
        .await?,
    })))
}

impl From<LibraryFavoriteAlbumsError> for actix_web::Error {
    fn from(err: LibraryFavoriteAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<LibraryAlbumOrder>,
    order_direction: Option<LibraryAlbumOrderDirection>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/favorites/albums",
        description = "List favorite albums",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<LibraryAlbumOrder>, Query, description = "Sort order"),
            ("orderDirection" = Option<LibraryAlbumOrderDirection>, Query, description = "Sort order direction"),
        ),
        responses(
            (
                status = 200,
                description = "Page of album metadata",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    query: web::Query<LibraryFavoriteAlbumsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        favorite_albums(
            &db,
            &AlbumsRequest {
                sort: match (query.order, query.order_direction) {
                    (None, None) => None,
                    (None, Some(direction)) => Some(match direction {
                        LibraryAlbumOrderDirection::Asc => AlbumSort::ReleaseDateAsc,
                        LibraryAlbumOrderDirection::Desc => AlbumSort::ReleaseDateDesc,
                    }),
                    (Some(order), None) => Some(match order {
                        LibraryAlbumOrder::Date => AlbumSort::ReleaseDateDesc,
                    }),
                    (Some(order), Some(direction)) => Some(match (order, direction) {
                        (LibraryAlbumOrder::Date, LibraryAlbumOrderDirection::Asc) => {
                            AlbumSort::ReleaseDateAsc
                        }
                        (LibraryAlbumOrder::Date, LibraryAlbumOrderDirection::Desc) => {
                            AlbumSort::ReleaseDateDesc
                        }
                    }),
                },
                page: if query.offset.is_some() || query.limit.is_some() {
                    Some(PagingRequest {
                        offset: query.offset.unwrap_or(0),
                        limit: query.limit.unwrap_or(10),
                    })
                } else {
                    None
                },
                ..Default::default()
            },
        )
        .await?
        .ok_try_into_map_err(|e| LibraryFavoriteAlbumsError::RequestFailed(format!("{e:?}")))?
        .into(),
    ))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/favorites/artists",
        description = "List favorite artists",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<LibraryArtistOrder>, Query, description = "Sort order"),
            ("orderDirection" = Option<LibraryArtistOrderDirection>, Query, description = "Sort order direction"),
        ),
        responses(
            (
                status = 200,
                description = "Page of artist metadata",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/artists", method = "GET")]
pub async fn favorite_artists_endpoint(
    query: web::Query<LibraryFavoriteArtistsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiArtist>>> {
    let artist: Page<LibraryArtist> = favorite_artists(
        &db,
        query.offset,
        query.limit,
        query.order,
        query.order_direction,
    )
    .await?
    .into();

    Ok(Json(artist.into()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        post,
        path = "/favorites/artists",
        description = "Add favorite artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "The artist ID"),
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
    query: web::Query<LibraryAddFavoriteArtistsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_artist(&db, &query.artist_id.into())?;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        delete,
        path = "/favorites/artists",
        description = "Delete favorite artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "The artist ID"),
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
    query: web::Query<LibraryRemoveFavoriteArtistsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_artist(&db, &query.artist_id.into())?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        post,
        path = "/favorites/albums",
        description = "Add favorite album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "The album ID"),
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
    query: web::Query<LibraryAddFavoriteAlbumsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_album(&db, &query.album_id.into())?;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        delete,
        path = "/favorites/albums",
        description = "Delete favorite album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "The album ID"),
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
    query: web::Query<LibraryRemoveFavoriteAlbumsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_album(&db, &query.album_id.into())?;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        post,
        path = "/favorites/tracks",
        description = "Add favorite track",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "The track ID"),
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
    query: web::Query<LibraryAddFavoriteTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    add_favorite_track(&db, &query.track_id.into())?;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        delete,
        path = "/favorites/tracks",
        description = "Delete favorite track",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "The track ID"),
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
    query: web::Query<LibraryRemoveFavoriteTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    remove_favorite_track(&db, &query.track_id.into())?;

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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/favorites/tracks",
        description = "List favorite tracks",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackIds" = Option<String>, Query, description = "A comma-separated list of track IDs"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("order" = Option<LibraryTrackOrder>, Query, description = "Sort order"),
            ("orderDirection" = Option<LibraryTrackOrderDirection>, Query, description = "Sort order direction"),
        ),
        responses(
            (
                status = 200,
                description = "List of artist album metadata",
                body = Value,
            )
        )
    )
)]
#[route("/favorites/tracks", method = "GET")]
pub async fn favorite_tracks_endpoint(
    query: web::Query<LibraryFavoriteTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let track_ids = query
        .track_ids
        .as_ref()
        .map(|ids| parse_integer_ranges_to_ids(ids.as_str()))
        .transpose()
        .map_err(|e| ErrorBadRequest(format!("Invalid track id values: {e:?}")))?;

    let tracks: Page<LibraryTrack> = favorite_tracks(
        &db,
        track_ids.as_deref(),
        query.offset,
        query.limit,
        query.order,
        query.order_direction,
    )
    .await?
    .into();

    Ok(Json(tracks.into()))
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<AlbumType> for LibraryAlbumType {
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
        tags = ["Library"],
        get,
        path = "/artists/albums",
        description = "Get the list of artist album metadata for an artistId",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "The artist ID"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("albumType" = Option<AlbumType>, Query, description = "Filter to this album type"),
        ),
        responses(
            (
                status = 200,
                description = "List of artist album metadata",
                body = Vec<ApiAlbum>,
            )
        )
    )
)]
#[route("/artists/albums", method = "GET")]
pub async fn artist_albums_endpoint(
    query: web::Query<LibraryArtistAlbumsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    Ok(Json(
        artist_albums(
            &db,
            &query.artist_id.into(),
            query.offset,
            query.limit,
            query.album_type.map(Into::into),
        )
        .await?
        .map(Into::into)
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/albums/tracks",
        description = "Get the list of album track metadata for an albumId",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "The album ID"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "List of album track metadata",
                body = Vec<ApiTrack>,
            )
        )
    )
)]
#[route("/albums/tracks", method = "GET")]
pub async fn album_tracks_endpoint(
    query: web::Query<LibraryAlbumTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiTrack>>> {
    let tracks: Page<LibraryTrack> =
        album_tracks(&db, &query.album_id.into(), query.offset, query.limit)
            .await?
            .into();

    Ok(Json(tracks.into()))
}

impl From<LibraryAlbumError> for actix_web::Error {
    fn from(err: LibraryAlbumError) -> Self {
        log::error!("{err:?}");
        match err {
            LibraryAlbumError::DatabaseFetch(_) => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAlbumQuery {
    album_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/albums",
        description = "Get the album metadata for an albumId",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = u64, Query, description = "The album ID"),
        ),
        responses(
            (
                status = 200,
                description = "Album metadata information",
                body = ApiAlbum,
            )
        )
    )
)]
#[route("/albums", method = "GET")]
pub async fn album_endpoint(
    query: web::Query<LibraryAlbumQuery>,
    db: LibraryDatabase,
) -> Result<Json<ApiAlbum>> {
    let album = album(&db, &query.album_id.into())
        .await?
        .ok_or_else(|| ErrorNotFound("Album not found"))?;

    Ok(Json(album.into()))
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

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/artists",
        description = "Get the artist metadata for an artistId",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = u64, Query, description = "The artist ID"),
        ),
        responses(
            (
                status = 200,
                description = "Artist metadata information",
                body = ApiArtist,
            )
        )
    )
)]
#[route("/artists", method = "GET")]
pub async fn artist_endpoint(
    query: web::Query<LibraryArtistQuery>,
    db: LibraryDatabase,
) -> Result<Json<ApiArtist>> {
    let artist = artist(&db, &query.artist_id.into()).await?;

    Ok(Json(artist.into()))
}

impl From<LibraryTrackError> for actix_web::Error {
    fn from(err: LibraryTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LibraryTrackQuery {
    track_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/tracks",
        description = "Get the track metadata for a trackId",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackId" = u64, Query, description = "The track ID"),
        ),
        responses(
            (
                status = 200,
                description = "Track metadata information",
                body = ApiTrack,
            )
        )
    )
)]
#[route("/tracks", method = "GET")]
pub async fn track_endpoint(
    query: web::Query<LibraryTrackQuery>,
    db: LibraryDatabase,
) -> Result<Json<ApiTrack>> {
    let track = track(&db, &query.track_id.into())
        .await?
        .ok_or_else(|| ErrorNotFound("Track not found"))?;

    Ok(Json(track.into()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchQuery {
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
    types: Option<Vec<SearchType>>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        get,
        path = "/search",
        description = "Search the library for artists/albums/tracks that fuzzy match the query",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("query" = String, Query, description = "The search query"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
            ("types" = Option<Vec<SearchType>>, Query, description = "List of types to filter the search by"),
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
    query: web::Query<LibrarySearchQuery>,
) -> Result<Json<ApiSearchResultsResponse>> {
    let results = search(
        &query.query,
        query.offset,
        query.limit,
        query
            .types
            .clone()
            .map(|x| x.into_iter().map(Into::into).collect::<Vec<_>>())
            .as_deref(),
    )
    .map_err(ErrorInternalServerError)?;

    Ok(Json(results))
}

impl From<ReindexError> for actix_web::Error {
    fn from(err: ReindexError) -> Self {
        log::error!("{err:?}");
        match err {
            ReindexError::DatabaseFetch(_)
            | ReindexError::RecreateIndex(_)
            | ReindexError::PopulateIndex(_)
            | ReindexError::GetAlbums(_) => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReindexQuery {}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Library"],
        post,
        path = "/reindex",
        description = "Reindex the search database with the complete library",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
#[route("/reindex", method = "POST")]
pub async fn reindex_endpoint(
    _query: web::Query<ReindexQuery>,
    db: LibraryDatabase,
) -> Result<Json<Value>> {
    reindex_global_search_index(&db).await?;

    Ok(Json(serde_json::json!({"success": true})))
}
