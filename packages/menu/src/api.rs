use std::str::FromStr;

use actix_web::{
    delete,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_core::{
    integer_range::parse_integer_ranges_to_ids,
    sqlite::models::{AlbumType, ApiAlbum, ApiArtist, ApiSource, Id, IdType},
};
use moosicbox_core::{
    integer_range::ParseIntegersError,
    sqlite::models::{AlbumSort, AlbumSource, ArtistSort, ToApi},
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_library::{
    db::{get_album_tracks, get_tracks},
    models::ApiTrack,
    LibraryMusicApi,
};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_music_api::{
    models::{AlbumFilters, AlbumsRequest},
    MusicApis, SourceToMusicApi as _,
};
use moosicbox_paging::{Page, PagingRequest};
use serde::Deserialize;
use thiserror::Error;

use crate::library::{
    albums::{add_album, get_album_versions_from_source, refavorite_album, remove_album},
    artists::{get_all_artists, ArtistFilters, ArtistsRequest},
    get_album_from_source, get_artist, get_artist_albums, get_library_album, GetArtistError,
};

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
        moosicbox_core::sqlite::models::TrackApiSource,
    ))
)]
pub struct Api;

fn album_id_for_source(id: &str, source: ApiSource) -> Result<Id, actix_web::Error> {
    Ok(match source {
        ApiSource::Library => id
            .parse::<i32>()
            .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?
            .into(),
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => id
            .parse::<u64>()
            .map_err(|_| ErrorBadRequest(format!("Bad Tidal album_id {id}")))?
            .into(),
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => id.to_string().into(),
        #[cfg(feature = "yt")]
        ApiSource::Yt => id.to_string().into(),
    })
}

#[derive(Debug, Error)]
pub enum MenuError {
    #[error(transparent)]
    BadRequest(#[from] actix_web::Error),
    #[error("Internal server error: {error:?}")]
    InternalServerError { error: String },
    #[error("Not Found Error: {error:?}")]
    NotFound { error: String },
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistsQuery {
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    search: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artists",
        description = "Get the artists for the specified criteria",
        params(
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
                    .map(|s| s.trim())
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    source: Option<ApiSource>,
    album_type: Option<AlbumType>,
    sources: Option<String>,
    sort: Option<String>,
    name: Option<String>,
    artist: Option<String>,
    search: Option<String>,
    artist_id: Option<String>,
    tidal_artist_id: Option<u64>,
    qobuz_artist_id: Option<u64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/albums",
        description = "Get the albums for the specified criteria",
        params(
            ("source" = Option<ApiSource>, Query, description = "API Source to fetch the albums from"),
            ("albumType" = Option<AlbumType>, Query, description = "Album type to filter by"),
            ("sources" = Option<String>, Query, description = "List of API sources to filter by"),
            ("sort" = Option<String>, Query, description = "Order to sort by"),
            ("name" = Option<String>, Query, description = "Name to filter by"),
            ("artist" = Option<String>, Query, description = "Artist name to filter by"),
            ("search" = Option<String>, Query, description = "Query to generically search by"),
            ("artistId" = Option<String>, Query, description = "Artist ID to filter by"),
            ("tidalArtistId" = Option<i32>, Query, description = "Tidal artist ID to filter by"),
            ("qobuzArtistId" = Option<i32>, Query, description = "Qobuz artist ID to filter by"),
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
) -> Result<Json<Page<ApiAlbum>>> {
    let source = query.source.unwrap_or(ApiSource::Library);

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
                    .map(|s| s.trim())
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
            artist_id: query
                .artist_id
                .as_ref()
                .and_then(|x| Id::try_from_str(x.as_str(), source, IdType::Artist).ok()),
            tidal_artist_id: query.tidal_artist_id.map(|x| x.into()),
            qobuz_artist_id: query.qobuz_artist_id.map(|x| x.into()),
        }),
    };

    let api = music_apis.get(source).map_err(ErrorBadRequest)?;

    Ok(Json(
        if let Some(artist_id) = request.filters.as_ref().and_then(|x| x.artist_id.as_ref()) {
            api.artist_albums(
                artist_id,
                query.album_type,
                query.offset,
                query.limit,
                None,
                None,
            )
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e}")))?
            .page
            .map(Into::into)
        } else {
            api.albums(&request)
                .await
                .map_err(|e| ErrorInternalServerError(format!("Failed to fetch albums: {e}")))?
                .page
                .map(Into::into)
        },
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTracksQuery {
    track_ids: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/tracks",
        description = "Get the tracks for the specified criteria",
        params(
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

    Ok(Json(
        get_tracks(&db, Some(&ids))
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch tracks"))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumTracksQuery {
    album_id: i32,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album/tracks",
        description = "Get the tracks for the specified album",
        params(
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
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumVersionsQuery {
    album_id: String,
    source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album/versions",
        description = "Get the album versions for the specified album",
        params(
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
) -> Result<Json<Vec<ApiAlbumVersion>>> {
    let source = query.source.unwrap_or(ApiSource::Library);
    let id = Id::try_from_str(&query.album_id, source, IdType::Album).map_err(ErrorBadRequest)?;
    Ok(Json(
        get_album_versions_from_source(&db, &library_api, &id, source)
            .await
            .map_err(|_e| ErrorInternalServerError("Failed to fetch album versions"))?
            .into_iter()
            .map(|t| t.to_api())
            .collect(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistAlbumsQuery {
    artist_id: i32,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artist/albums",
        description = "Get the albums for the specified artist",
        params(
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
            .map_err(|_e| ErrorInternalServerError("Failed to fetch albums"))?
            .iter()
            .map(Into::into)
            .collect(),
    ))
}

impl From<GetArtistError> for actix_web::Error {
    fn from(e: GetArtistError) -> Self {
        match e {
            GetArtistError::Artist(_) => ErrorInternalServerError(e),
            GetArtistError::InvalidRequest => {
                ErrorBadRequest(format!("Failed to fetch artist: {e:?}"))
            }
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetArtistQuery {
    artist_id: Option<String>,
    album_id: Option<String>,
    source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/artist",
        description = "Get the artist for the specified criteria",
        params(
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
    let source = query.source.unwrap_or(ApiSource::Library);
    let api = music_apis.get(source).map_err(ErrorBadRequest)?;
    Ok(Json(
        get_artist(
            &**api,
            query
                .artist_id
                .as_ref()
                .and_then(|x| Id::try_from_str(x, source, IdType::Artist).ok())
                .as_ref(),
            query
                .album_id
                .as_ref()
                .and_then(|x| Id::try_from_str(x, source, IdType::Album).ok())
                .as_ref(),
        )
        .await?
        .ok_or_else(|| ErrorNotFound("Artist not found"))?
        .into(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumQuery {
    album_id: Option<String>,
    tidal_album_id: Option<String>,
    qobuz_album_id: Option<String>,
    source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        get,
        path = "/album",
        description = "Get the album for the specified criteria",
        params(
            ("albumId" = Option<String>, Query, description = "Album ID to filter by"),
            ("tidalAlbumId" = Option<String>, Query, description = "Tidal album ID to filter by"),
            ("qobuzAlbumId" = Option<String>, Query, description = "Qobuz album ID to filter by"),
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
    db: LibraryDatabase,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(if let Some(id) = &query.album_id {
        let source = query.source.unwrap_or(ApiSource::Library);
        let id = Id::try_from_str(id, source, IdType::Album).map_err(ErrorBadRequest)?;
        get_album_from_source(&db, &id, source)
            .await
            .map_err(ErrorInternalServerError)?
            .ok_or(ErrorNotFound("Album not found"))?
            .into()
    } else {
        #[allow(unused)]
        let (id, source) = if let Some(id) = &query.tidal_album_id {
            #[cfg(feature = "tidal")]
            {
                (
                    Id::try_from_str(id, ApiSource::Tidal, IdType::Album)
                        .map_err(ErrorBadRequest)?,
                    ApiSource::Tidal,
                )
            }
            #[cfg(not(feature = "tidal"))]
            return Err(ErrorNotFound("Unsupported ApiSource"));
        } else if let Some(id) = &query.qobuz_album_id {
            #[cfg(feature = "qobuz")]
            {
                (
                    Id::try_from_str(id, ApiSource::Qobuz, IdType::Album)
                        .map_err(ErrorBadRequest)?,
                    ApiSource::Qobuz,
                )
            }
            #[cfg(not(feature = "qobuz"))]
            return Err(ErrorNotFound("Unsupported ApiSource"));
        } else {
            return Err(ErrorNotFound("Album not found"));
        };

        #[allow(unreachable_code)]
        {
            get_library_album(&db, &id, source)
                .await
                .map_err(ErrorInternalServerError)?
                .ok_or(ErrorNotFound("Album not found"))?
                .into()
        }
    }))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        post,
        path = "/album",
        description = "Add the album to the library",
        params(
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
pub async fn add_album_endpoint(
    query: web::Query<AddAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        add_album(
            &**music_apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add album: {e:?}")))?
        .into(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        delete,
        path = "/album",
        description = "Add the album to the library",
        params(
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
pub async fn remove_album_endpoint(
    query: web::Query<RemoveAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        remove_album(
            &**music_apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove album: {e:?}")))?
        .into(),
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReFavoriteAlbumQuery {
    album_id: String,
    source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Menu"],
        delete,
        path = "/album/re-favorite",
        description = "Re-favorite the album on the given API source",
        params(
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
pub async fn refavorite_album_endpoint(
    query: web::Query<ReFavoriteAlbumQuery>,
    db: LibraryDatabase,
    library_api: LibraryMusicApi,
    music_apis: MusicApis,
) -> Result<Json<ApiAlbum>> {
    Ok(Json(
        refavorite_album(
            &**music_apis
                .get(query.source)
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &library_api,
            &db,
            &album_id_for_source(&query.album_id, query.source)?,
        )
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to re-favorite album: {e:?}")))?
        .into(),
    ))
}
