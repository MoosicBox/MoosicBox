//! HTTP API endpoints for menu operations.
//!
//! This module provides actix-web HTTP endpoints for querying and managing music
//! library content, including artists, albums, and tracks. It handles request
//! parsing, validation, and delegates to the library module for business logic.

#![allow(clippy::needless_for_each)]

use std::str::FromStr;

use actix_web::{
    Result, Scope, delete,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
};
use moosicbox_library::db::{get_album_tracks, get_tracks};
use moosicbox_library_music_api::LibraryMusicApi;
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_music_api::{
    MusicApis, SourceToMusicApi as _,
    models::{AlbumFilters, AlbumsRequest},
};
use moosicbox_music_models::{
    AlbumSort, AlbumSource, AlbumType, ApiSource, ArtistSort,
    api::{ApiAlbum, ApiArtist, ApiTrack},
    id::{ApiId, Id, ParseIntegersError, parse_integer_ranges_to_ids},
};
use moosicbox_paging::{Page, PagingRequest};
use moosicbox_profiles::api::ProfileName;
use serde::Deserialize;
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;

use crate::library::{
    GetArtistError,
    albums::{
        add_album, get_album_versions_from_source, get_albums_from_source, refavorite_album,
        remove_album,
    },
    artists::{ArtistFilters, ArtistsRequest, get_all_artists},
    get_album_from_source, get_artist, get_artist_albums,
};

/// Binds all menu API endpoints to the provided actix-web scope.
///
/// This function registers all HTTP endpoints for menu operations, including
/// artist queries, album management, track retrieval, and album version management.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(get_artists_endpoint)
        .service(get_artist_endpoint)
        .service(get_album_endpoint)
        .service(add_album_endpoint)
        .service(remove_album_endpoint)
        .service(refavorite_album_endpoint)
        .service(get_albums_endpoint)
        .service(get_tracks_endpoint)
        .service(get_album_tracks_endpoint)
        .service(get_album_versions_endpoint)
        .service(get_artist_albums_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Menu")),
    paths(
        get_artists_endpoint,
        get_albums_endpoint,
        get_tracks_endpoint,
        get_album_tracks_endpoint,
        get_album_versions_endpoint,
        get_artist_albums_endpoint,
        get_artist_endpoint,
        get_album_endpoint,
        add_album_endpoint,
        remove_album_endpoint,
        refavorite_album_endpoint,
    ),
    components(schemas(
        ApiAlbum,
        ApiArtist,
        ApiTrack,
        ApiAlbumVersion,
        moosicbox_music_models::TrackApiSource,
    ))
)]
/// `OpenAPI` documentation configuration for menu API endpoints.
pub struct Api;

/// Converts an album ID string to the appropriate ID type for the given source.
///
/// For library sources, parses the ID as an integer. For external API sources,
/// uses the string ID as-is.
///
/// # Errors
///
/// * `ErrorBadRequest` if the ID cannot be parsed as an integer for library sources
fn album_id_for_source(id: &str, source: &ApiSource) -> Result<Id, actix_web::Error> {
    Ok(if source.is_library() {
        id.parse::<i32>()
            .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?
            .into()
    } else {
        id.to_string().into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_album_id_for_source_library_valid() {
        let source = ApiSource::library();
        let result = album_id_for_source("123", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::Number(123));
    }

    #[test_log::test]
    fn test_album_id_for_source_library_invalid() {
        let source = ApiSource::library();
        let result = album_id_for_source("not_a_number", &source);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_album_id_for_source_library_empty() {
        let source = ApiSource::library();
        let result = album_id_for_source("", &source);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_album_id_for_source_tidal() {
        let source = ApiSource::register("Tidal", "Tidal");
        let result = album_id_for_source("tidal123", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::String("tidal123".to_string()));
    }

    #[test_log::test]
    fn test_album_id_for_source_qobuz() {
        let source = ApiSource::register("Qobuz", "Qobuz");
        let result = album_id_for_source("qobuz456", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::String("qobuz456".to_string()));
    }

    #[test_log::test]
    fn test_album_id_for_source_external_with_numeric_string() {
        let source = ApiSource::register("Tidal", "Tidal");
        let result = album_id_for_source("999", &source);
        assert!(result.is_ok());
        // For external sources, numeric strings remain as strings
        assert_eq!(result.unwrap(), Id::String("999".to_string()));
    }

    #[test_log::test]
    fn test_get_artist_error_conversion_invalid_request() {
        let error = GetArtistError::InvalidRequest;
        let actix_error: actix_web::Error = error.into();
        // Should convert to bad request
        assert_eq!(
            actix_error.as_response_error().status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
    }

    #[test_log::test]
    fn test_get_artist_error_conversion_music_api() {
        let music_api_error = moosicbox_music_api::Error::Unauthorized;
        let error = GetArtistError::MusicApi(music_api_error);
        let actix_error: actix_web::Error = error.into();
        // Should convert to internal server error
        assert_eq!(
            actix_error.as_response_error().status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test_log::test]
    fn test_album_id_for_source_library_negative_number() {
        let source = ApiSource::library();
        let result = album_id_for_source("-42", &source);
        // Negative numbers are valid i32 values and will be cast to u64
        assert!(result.is_ok());
        #[allow(clippy::cast_sign_loss)]
        let expected = -42i32 as u64;
        assert_eq!(result.unwrap(), Id::Number(expected));
    }

    #[test_log::test]
    fn test_album_id_for_source_library_max_i32() {
        let source = ApiSource::library();
        let result = album_id_for_source("2147483647", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::Number(i32::MAX as u64));
    }

    #[test_log::test]
    fn test_album_id_for_source_library_overflow() {
        let source = ApiSource::library();
        // i32::MAX + 1 should fail to parse
        let result = album_id_for_source("2147483648", &source);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_album_id_for_source_library_with_whitespace() {
        let source = ApiSource::library();
        // Whitespace should cause parse failure
        let result = album_id_for_source(" 123 ", &source);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_album_id_for_source_library_float() {
        let source = ApiSource::library();
        // Float should cause parse failure for library source
        let result = album_id_for_source("123.45", &source);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_album_id_for_source_external_with_special_characters() {
        let source = ApiSource::register("Tidal", "Tidal");
        let result = album_id_for_source("album:123/456?test=true", &source);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Id::String("album:123/456?test=true".to_string())
        );
    }

    #[test_log::test]
    fn test_album_id_for_source_external_with_unicode() {
        let source = ApiSource::register("Qobuz", "Qobuz");
        let result = album_id_for_source("アルバム123", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::String("アルバム123".to_string()));
    }

    #[test_log::test]
    fn test_album_id_for_source_external_empty_string() {
        let source = ApiSource::register("Tidal", "Tidal");
        let result = album_id_for_source("", &source);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Id::String(String::new()));
    }
}

/// Error types that can occur during menu API operations.
#[derive(Debug, Error)]
pub enum MenuError {
    /// Bad request from client
    #[error(transparent)]
    BadRequest(#[from] actix_web::Error),
    /// Internal server error
    #[error("Internal server error: {error:?}")]
    InternalServerError {
        /// Error message
        error: String,
    },
    /// Resource not found
    #[error("Not Found Error: {error:?}")]
    NotFound {
        /// Error message
        error: String,
    },
}

/// Query parameters for retrieving artists.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistsQuery {
    /// Comma-separated list of API sources to filter by
    sources: Option<String>,
    /// Sort order for the results
    sort: Option<String>,
    /// Filter by artist name
    name: Option<String>,
    /// Generic search query
    search: Option<String>,
}

/// HTTP endpoint for retrieving artists based on query criteria.
///
/// Returns a list of artists filtered and sorted according to the query parameters.
///
/// # Errors
///
/// * `ErrorBadRequest` if the query parameters are invalid
/// * `ErrorInternalServerError` if the database query fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artists",
        description = "Get the artists for the specified criteria",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("sources" = Option<String>, Query, description = "List of API sources to filter by"),
            ("sort" = Option<String>, Query, description = "Order to sort by"),
            ("name" = Option<String>, Query, description = "Name to filter by"),
            ("search" = Option<String>, Query, description = "Query to generically search by"),
        ),
        responses(
            (
                status = 200,
                description = "The list of artists",
                body = Vec<ApiArtist>,
            )
        )
    )
)]
#[get("/artists")]
pub async fn get_artists_endpoint(
    query: web::Query<GetArtistsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Vec<ApiArtist>>> {
    let request = ArtistsRequest {
        sources: query
            .sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(str::trim)
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: query
            .sort
            .as_ref()
            .map(|sort| {
                ArtistSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
        filters: ArtistFilters {
            name: query.name.clone().map(|s| s.to_lowercase()),
            search: query.search.clone().map(|s| s.to_lowercase()),
        },
    };

    Ok(Json(
        get_all_artists(&db, &request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch artists: {e:?}")))?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

/// Query parameters for retrieving albums.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    /// API source to fetch albums from
    source: Option<ApiSource>,
    /// Filter by album type
    album_type: Option<AlbumType>,
    /// Comma-separated list of API sources to filter by
    sources: Option<String>,
    /// Sort order for the results
    sort: Option<String>,
    /// Filter by album name
    name: Option<String>,
    /// Filter by artist name
    artist: Option<String>,
    /// Generic search query
    search: Option<String>,
    /// Filter by artist ID
    artist_id: Option<String>,
    /// API source for artist ID lookup
    api_source: Option<ApiSource>,
    /// Page offset for pagination
    offset: Option<u32>,
    /// Page limit for pagination
    limit: Option<u32>,
}

/// HTTP endpoint for retrieving albums based on query criteria.
///
/// Returns a paginated list of albums filtered and sorted according to the query parameters.
///
/// # Errors
///
/// * `ErrorBadRequest` if the query parameters are invalid or the source is invalid
/// * `ErrorInternalServerError` if fetching albums from the API fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/albums",
        description = "Get the albums for the specified criteria",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("sources" = Option<ApiSource>, Query, description = "ApiSources to fetch the albums from"),
            ("albumType" = Option<AlbumType>, Query, description = "Album type to filter by"),
            ("sources" = Option<String>, Query, description = "List of API sources to filter by"),
            ("sort" = Option<String>, Query, description = "Order to sort by"),
            ("name" = Option<String>, Query, description = "Name to filter by"),
            ("artist" = Option<String>, Query, description = "Artist name to filter by"),
            ("search" = Option<String>, Query, description = "Query to generically search by"),
            ("artistId" = Option<String>, Query, description = "Artist ID to filter by"),
            ("apiSource" = Option<ApiSource>, Query, description = "ApiSource to search the artist by"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "The list of albums",
                body = Value,
            )
        )
    )
)]
#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    music_apis: MusicApis,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiAlbum>>> {
    let source = query.source.clone().unwrap_or_else(ApiSource::library);
    let artist_id = query
        .artist_id
        .as_ref()
        .and_then(|x| Id::try_from_str(x.as_str(), &source).ok());

    let request = AlbumsRequest {
        page: if query.offset.is_some() || query.limit.is_some() {
            Some(PagingRequest {
                offset: query.offset.unwrap_or(0),
                limit: query.limit.unwrap_or(100),
            })
        } else {
            None
        },
        sources: query
            .sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(str::trim)
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: query
            .sort
            .as_ref()
            .map(|sort| {
                AlbumSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
        filters: Some(AlbumFilters {
            name: query.name.clone().map(|s| s.to_lowercase()),
            artist: query.artist.clone().map(|s| s.to_lowercase()),
            search: query.search.clone().map(|s| s.to_lowercase()),
            album_type: query.album_type,
            artist_id: artist_id.clone(),
            artist_api_id: query
                .api_source
                .as_ref()
                .map(|x| {
                    artist_id
                        .ok_or_else(|| ErrorBadRequest("Invalid artist_id"))
                        .map(|id| ApiId {
                            source: x.clone(),
                            id,
                        })
                })
                .transpose()?,
        }),
    };

    let api = music_apis
        .get(&source)
        .ok_or_else(|| ErrorBadRequest("Invalid source"))?;

    Ok(Json(
        get_albums_from_source(&db, &**api, request)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e}")))?
            .map(Into::into),
    ))
}

/// Query parameters for retrieving tracks.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTracksQuery {
    /// Comma-separated list of track IDs to fetch
    track_ids: String,
}

/// HTTP endpoint for retrieving tracks by their IDs.
///
/// Returns a list of tracks in the order specified by the track IDs parameter.
///
/// # Errors
///
/// * `ErrorBadRequest` if the track IDs cannot be parsed
/// * `ErrorInternalServerError` if the database query fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/tracks",
        description = "Get the tracks for the specified criteria",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("trackIds" = String, Query, description = "Comma-separated list of track IDs to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "The list of tracks",
                body = Vec<ApiTrack>,
            )
        )
    )
)]
#[get("/tracks")]
pub async fn get_tracks_endpoint(
    query: web::Query<GetTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Vec<ApiTrack>>> {
    let ids = parse_integer_ranges_to_ids(&query.track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    let mut tracks = get_tracks(&db, Some(&ids))
        .await
        .map_err(|_e| ErrorInternalServerError("Failed to fetch tracks"))?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<ApiTrack>>();

    let mut sorted_tracks = Vec::with_capacity(tracks.len());

    for id in ids {
        if let Some(index) = tracks.iter().position(|x| x.track_id == id) {
            sorted_tracks.push(tracks.remove(index));
        }
    }

    Ok(Json(sorted_tracks))
}

/// Query parameters for retrieving album tracks.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumTracksQuery {
    /// Album ID to fetch tracks for
    album_id: i32,
}

/// HTTP endpoint for retrieving all tracks in a specific album.
///
/// Returns the tracks belonging to the specified album ID.
///
/// # Errors
///
/// * `ErrorInternalServerError` if the database query fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album/tracks",
        description = "Get the tracks for the specified album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to fetch the tracks for"),
        ),
        responses(
            (
                status = 200,
                description = "The list of tracks",
                body = Vec<ApiTrack>,
            )
        )
    )
)]
#[get("/album/tracks")]
pub async fn get_album_tracks_endpoint(
    query: web::Query<GetAlbumTracksQuery>,
    db: LibraryDatabase,
) -> Result<Json<Vec<ApiTrack>>> {
    Ok(Json(
        get_album_tracks(&db, &query.album_id.into())
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch tracks"))?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

/// Query parameters for retrieving album versions.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumVersionsQuery {
    /// Album ID to fetch versions for
    album_id: String,
    /// API source for the album
    source: Option<ApiSource>,
}

/// HTTP endpoint for retrieving different versions of an album.
///
/// Returns a list of album versions (e.g., different quality formats) for the specified album.
///
/// # Errors
///
/// * `ErrorBadRequest` if the album ID is invalid
/// * `ErrorInternalServerError` if fetching album versions fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album/versions",
        description = "Get the album versions for the specified album",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to fetch the versions for"),
            ("source" = Option<ApiSource>, Query, description = "Album source to retrieve"),
        ),
        responses(
            (
                status = 200,
                description = "The list of album versions",
                body = Vec<ApiAlbumVersion>,
            )
        )
    )
)]
#[get("/album/versions")]
pub async fn get_album_versions_endpoint(
    query: web::Query<GetAlbumVersionsQuery>,
    library_api: LibraryMusicApi,
    db: LibraryDatabase,
    profile: ProfileName,
) -> Result<Json<Vec<ApiAlbumVersion>>> {
    let source = query.source.clone().unwrap_or_else(ApiSource::library);
    let id = Id::try_from_str(&query.album_id, &source).map_err(ErrorBadRequest)?;
    Ok(Json(
        get_album_versions_from_source(&db, &library_api, profile.as_ref(), &id, source)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch album versions"))?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

/// Query parameters for retrieving artist albums.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistAlbumsQuery {
    /// Artist ID to fetch albums for
    artist_id: i32,
}

/// HTTP endpoint for retrieving all albums by a specific artist.
///
/// Returns the albums associated with the specified artist ID.
///
/// # Errors
///
/// * `ErrorInternalServerError` if the database query fails or album conversion fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artist/albums",
        description = "Get the albums for the specified artist",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = String, Query, description = "Artist ID to fetch the albums for"),
        ),
        responses(
            (
                status = 200,
                description = "The list of albums",
                body = Vec<ApiAlbum>,
            )
        )
    )
)]
#[get("/artist/albums")]
pub async fn get_artist_albums_endpoint(
    query: web::Query<GetArtistAlbumsQuery>,
    db: LibraryDatabase,
) -> Result<Json<Vec<ApiAlbum>>> {
    Ok(Json(
        get_artist_albums(&query.artist_id.into(), &db)
            .await
            .map_err(ErrorInternalServerError)?
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()
            .map_err(ErrorInternalServerError)?,
    ))
}

impl From<GetArtistError> for actix_web::Error {
    fn from(e: GetArtistError) -> Self {
        match e {
            GetArtistError::MusicApi(_) => ErrorInternalServerError(e),
            GetArtistError::InvalidRequest => {
                ErrorBadRequest(format!("Failed to fetch artist: {e:?}"))
            }
        }
    }
}

/// Query parameters for retrieving an artist.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistQuery {
    /// Artist ID to fetch
    artist_id: Option<String>,
    /// Album ID to fetch artist from
    album_id: Option<String>,
    /// API source for the artist
    source: Option<ApiSource>,
}

/// HTTP endpoint for retrieving a specific artist.
///
/// Returns an artist by ID or by album association, depending on query parameters.
///
/// # Errors
///
/// * `ErrorBadRequest` if the source is invalid
/// * `ErrorNotFound` if the artist is not found
/// * Errors from the music API if artist retrieval fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artist",
        description = "Get the artist for the specified criteria",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("artistId" = Option<String>, Query, description = "Artist ID to filter by"),
            ("albumId" = Option<String>, Query, description = "Album ID to filter by"),
            ("source" = Option<ApiSource>, Query, description = "Artist source to retrieve"),
        ),
        responses(
            (
                status = 200,
                description = "The matching artist",
                body = ApiArtist,
            )
        )
    )
)]
#[get("/artist")]
pub async fn get_artist_endpoint(
    query: web::Query<GetArtistQuery>,
    music_apis: MusicApis,
) -> Result<Json<ApiArtist>> {
    let source = query.source.clone().unwrap_or_else(ApiSource::library);
    let api = music_apis
        .get(&source)
        .ok_or_else(|| ErrorBadRequest("Invalid source"))?;
    Ok(Json(
        get_artist(
            &**api,
            query
                .artist_id
                .as_ref()
                .and_then(|x| Id::try_from_str(x, &source).ok())
                .as_ref(),
            query
                .album_id
                .as_ref()
                .and_then(|x| Id::try_from_str(x, &source).ok())
                .as_ref(),
        )
        .await?
        .ok_or_else(|| ErrorNotFound("Artist not found"))?
        .into(),
    ))
}

/// Query parameters for retrieving an album.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumQuery {
    /// Album ID to fetch
    album_id: String,
    /// API source for the album
    source: Option<ApiSource>,
}

/// HTTP endpoint for retrieving a specific album.
///
/// Returns an album by ID from the specified source.
///
/// # Errors
///
/// * `ErrorBadRequest` if the album ID is invalid
/// * `ErrorNotFound` if the album is not found
/// * `ErrorInternalServerError` if fetching the album fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album",
        description = "Get the album for the specified criteria",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to filter by"),
            ("source" = Option<ApiSource>, Query, description = "Album source to retrieve"),
        ),
        responses(
            (
                status = 200,
                description = "The matching album",
                body = Value,
            )
        )
    )
)]
#[get("/album")]
pub async fn get_album_endpoint(
    query: web::Query<GetAlbumQuery>,
    profile: ProfileName,
    db: LibraryDatabase,
) -> Result<Json<ApiAlbum>> {
    let source = query.source.clone().unwrap_or_else(ApiSource::library);
    let id = Id::try_from_str(&query.album_id, &source).map_err(ErrorBadRequest)?;

    Ok(Json(
        get_album_from_source(&db, profile.as_ref(), &id, &source)
            .await
            .map_err(ErrorInternalServerError)?
            .ok_or_else(|| ErrorNotFound("Album not found"))?
            .into(),
    ))
}

/// Query parameters for adding an album to the library.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddAlbumQuery {
    /// Album ID to add
    album_id: String,
    /// API source for the album
    source: ApiSource,
}

/// HTTP endpoint for adding an album to the library.
///
/// Adds an album from an external source to the local library, including all tracks.
///
/// # Errors
///
/// * `ErrorBadRequest` if the album ID is invalid or the source is invalid
/// * `ErrorInternalServerError` if adding the album fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        post,
        path = "/album",
        description = "Add the album to the library",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to add"),
            ("source" = ApiSource, Query, description = "The API source to add the album from"),
        ),
        responses(
            (
                status = 200,
                description = "The added album",
                body = ApiAlbum,
            )
        )
    )
)]
#[post("/album")]
#[allow(clippy::future_not_send)]
pub async fn add_album_endpoint(
    query: web::Query<AddAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        add_album(
            &**music_apis
                .get(&query.source)
                .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, &query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add album: {e:?}")))?
        .into(),
    ))
}

/// Query parameters for removing an album from the library.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAlbumQuery {
    /// Album ID to remove
    album_id: String,
    /// API source for the album
    source: ApiSource,
}

/// HTTP endpoint for removing an album from the library.
///
/// Removes an album from the library, including its tracks and search index entries.
///
/// # Errors
///
/// * `ErrorBadRequest` if the album ID is invalid or the source is invalid
/// * `ErrorInternalServerError` if removing the album fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        delete,
        path = "/album",
        description = "Add the album to the library",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to remove"),
            ("source" = ApiSource, Query, description = "The API source the album existed in"),
        ),
        responses(
            (
                status = 200,
                description = "The removed album",
                body = ApiAlbum,
            )
        )
    )
)]
#[delete("/album")]
#[allow(clippy::future_not_send)]
pub async fn remove_album_endpoint(
    query: web::Query<RemoveAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        remove_album(
            &**music_apis
                .get(&query.source)
                .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, &query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove album: {e:?}")))?
        .into(),
    ))
}

/// Query parameters for re-favoriting an album.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReFavoriteAlbumQuery {
    /// Album ID to re-favorite
    album_id: String,
    /// API source for the album
    source: ApiSource,
}

/// HTTP endpoint for re-favoriting an album.
///
/// Removes and re-adds an album from an external source to update it with the latest version.
///
/// # Errors
///
/// * `ErrorBadRequest` if the album ID is invalid or the source is invalid
/// * `ErrorInternalServerError` if re-favoriting the album fails
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        delete,
        path = "/album/re-favorite",
        description = "Re-favorite the album on the given API source",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("albumId" = String, Query, description = "Album ID to re-favorite"),
            ("source" = ApiSource, Query, description = "The API source the album exists in"),
        ),
        responses(
            (
                status = 200,
                description = "The re-favorited album",
                body = ApiAlbum,
            )
        )
    )
)]
#[post("/album/re-favorite")]
#[allow(clippy::future_not_send)]
pub async fn refavorite_album_endpoint(
    query: web::Query<ReFavoriteAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        refavorite_album(
            &**music_apis
                .get(&query.source)
                .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, &query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to re-favorite album: {e:?}")))?
        .into(),
    ))
}
