//! HTTP endpoint handlers for music API operations.
//!
//! This module provides Actix-Web route handlers for managing music service providers,
//! including listing APIs, authentication, library scanning, and search functionality.
//! All endpoints are profile-aware and operate within the context of a `MoosicBox` profile.

use std::collections::BTreeMap;

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
};
use moosicbox_music_api::{
    MusicApis, SourceToMusicApi as _, models::search::api::ApiSearchResultsResponse,
};
use moosicbox_music_models::ApiSource;
use moosicbox_paging::Page;
use moosicbox_profiles::api::ProfileName;
use serde::Deserialize;

use crate::models::{ApiMusicApi, AuthValues, convert_to_api_music_api};

/// Binds music API HTTP endpoints to an Actix-Web scope.
///
/// Registers all music API-related endpoints including listing APIs,
/// authentication, scanning, and search functionality.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(music_apis_endpoint)
        .service(auth_music_api_endpoint)
        .service(scan_music_api_endpoint)
        .service(enable_scan_origin_music_api_endpoint)
        .service(search_music_apis_endpoint)
}

/// `OpenAPI` specification for music API endpoints.
///
/// Provides `OpenAPI`/Swagger documentation for all music API-related endpoints.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "MusicApi")),
    paths(
        music_apis_endpoint,
        auth_music_api_endpoint,
        scan_music_api_endpoint,
        enable_scan_origin_music_api_endpoint,
        search_music_apis_endpoint,
    ),
    components(schemas(
        ApiMusicApi,
    ))
)]
pub struct Api;

/// Query parameters for retrieving music APIs.
///
/// Used to paginate the list of enabled music APIs for a profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMusicApis {
    /// Starting offset for pagination (default: 0)
    offset: Option<u32>,
    /// Maximum number of items to return (default: 30)
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["MusicApi"],
        get,
        path = "",
        description = "Get the list enabled music APIs",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of tracks for the music APIs",
                body = Value,
            )
        )
    )
)]
#[route("", method = "GET")]
/// Retrieves a paginated list of enabled music APIs for the current profile.
///
/// # Errors
///
/// * Returns a 404 error if the specified profile is not found
/// * Returns a 500 error if querying music API state fails
///
/// # Panics
///
/// * Panics if the number of music APIs exceeds `u32::MAX` (practically impossible)
pub async fn music_apis_endpoint(
    query: web::Query<GetMusicApis>,
    profile_name: ProfileName,
) -> Result<Json<Page<ApiMusicApi>>> {
    let profile_name: String = profile_name.into();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let music_apis = moosicbox_music_api::profiles::PROFILES
        .get(&profile_name)
        .ok_or_else(|| ErrorNotFound(format!("Missing profile '{profile_name}'")))?;
    let music_apis = music_apis.iter().collect::<Vec<_>>();
    let total = u32::try_from(music_apis.len()).unwrap();
    let music_apis = futures::future::join_all(
        music_apis
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(convert_to_api_music_api),
    )
    .await;
    let music_apis = music_apis
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(ErrorInternalServerError)?;

    Ok(Json(Page::WithTotal {
        items: music_apis,
        offset,
        limit,
        total,
    }))
}

/// Query parameters for authenticating a music API.
///
/// Specifies which music API source to authenticate with.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMusicApi {
    /// The music API source to authenticate
    api_source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["MusicApi"],
        post,
        path = "/auth",
        description = "Authenticate a specific MusicApi",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("apiSource" = ApiSource, Query, description = "ApiSource to authenticate"),
        ),
        responses(
            (
                status = 200,
                description = "The updated state for the MusicApi that was authenticated",
                body = ApiMusicApi,
            )
        )
    )
)]
#[route("/auth", method = "POST")]
#[cfg_attr(not(feature = "_auth"), allow(unreachable_code, unused))]
/// Authenticates with a specific music API using the provided credentials.
///
/// Supports different authentication methods (username/password or OAuth polling)
/// depending on the API's capabilities.
///
/// # Errors
///
/// * Returns a 404 error if the specified music API is not found
/// * Returns a 400 error if the authentication method is not supported
/// * Returns a 500 error if the authentication process fails
pub async fn auth_music_api_endpoint(
    query: web::Query<AuthMusicApi>,
    form: web::Form<AuthValues>,
    music_apis: MusicApis,
) -> Result<Json<ApiMusicApi>> {
    let music_api = music_apis
        .get(&query.api_source)
        .ok_or_else(|| ErrorNotFound(format!("MusicApi '{}' not found", query.api_source)))?;

    match form.0 {
        AuthValues::UsernamePassword { username, password } => {
            #[cfg(not(feature = "auth-username-password"))]
            return Err(ErrorBadRequest("Auth not supported"));

            #[cfg(feature = "auth-username-password")]
            if let Some(auth) = music_api.auth() {
                let user_pass_auth = auth
                    .clone()
                    .into_username_password()
                    .ok_or_else(|| ErrorBadRequest("Auth not supported"))?;

                auth.attempt_login(move |_| {
                    let user_pass_auth = user_pass_auth.clone();
                    let username = username.clone();
                    let password = password.clone();
                    async move { user_pass_auth.login(username, password).await }
                })
                .await
                .map_err(ErrorInternalServerError)?;
            }
        }
        AuthValues::Poll => {
            #[cfg(not(feature = "auth-poll"))]
            return Err(ErrorBadRequest("Auth not supported"));

            #[cfg(feature = "auth-poll")]
            if let Some(auth) = music_api.auth() {
                let poll_auth = auth
                    .clone()
                    .into_poll()
                    .ok_or_else(|| ErrorBadRequest("Auth not supported"))?;

                auth.attempt_login(move |_| {
                    let poll_auth = poll_auth.clone();
                    async move { poll_auth.login().await }
                })
                .await
                .map_err(ErrorInternalServerError)?;
            }
        }
    }

    Ok(Json(
        convert_to_api_music_api(&**music_api)
            .await
            .map_err(ErrorInternalServerError)?,
    ))
}

/// Query parameters for scanning a music API.
///
/// Specifies which music API source to scan.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanMusicApi {
    /// The music API source to scan
    api_source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["MusicApi"],
        post,
        path = "/scan",
        description = "Scan a specific MusicApi",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("apiSource" = ApiSource, Query, description = "ApiSource to scan"),
        ),
        responses(
            (
                status = 200,
                description = "The updated state for the MusicApi that was scanned",
                body = ApiMusicApi,
            )
        )
    )
)]
#[route("/scan", method = "POST")]
/// Initiates a library scan for a specific music API.
///
/// This triggers the music API to scan and index its available content.
///
/// # Errors
///
/// * Returns a 404 error if the specified music API is not found
/// * Returns a 500 error if the scan operation fails
pub async fn scan_music_api_endpoint(
    query: web::Query<ScanMusicApi>,
    music_apis: MusicApis,
) -> Result<Json<ApiMusicApi>> {
    let music_api = music_apis
        .get(&query.api_source)
        .ok_or_else(|| ErrorNotFound(format!("MusicApi '{}' not found", query.api_source)))?;

    music_api.scan().await.map_err(ErrorInternalServerError)?;

    Ok(Json(
        convert_to_api_music_api(&**music_api)
            .await
            .map_err(ErrorInternalServerError)?,
    ))
}

/// Query parameters for enabling scan for a music API.
///
/// Specifies which music API source to enable for library scanning.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanMusicApi {
    /// The music API source to enable for scanning
    api_source: ApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["MusicApi"],
        post,
        path = "/scan-origins",
        description = "Enable a specific MusicApi scan origin",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("apiSource" = ApiSource, Query, description = "ApiSource to scan"),
        ),
        responses(
            (
                status = 200,
                description = "The updated state for the MusicApi that was enabled",
                body = ApiMusicApi,
            )
        )
    )
)]
#[route("/scan-origins", method = "POST")]
/// Enables library scanning for a specific music API.
///
/// This configures the music API to be included in library scan operations.
///
/// # Errors
///
/// * Returns a 404 error if the specified music API is not found
/// * Returns a 500 error if enabling the scan fails
pub async fn enable_scan_origin_music_api_endpoint(
    query: web::Query<EnableScanMusicApi>,
    music_apis: MusicApis,
) -> Result<Json<ApiMusicApi>> {
    let music_api = music_apis
        .get(&query.api_source)
        .ok_or_else(|| ErrorNotFound(format!("MusicApi '{}' not found", query.api_source)))?;

    music_api
        .enable_scan()
        .await
        .map_err(ErrorInternalServerError)?;

    Ok(Json(
        convert_to_api_music_api(&**music_api)
            .await
            .map_err(ErrorInternalServerError)?,
    ))
}

/// Query parameters for searching music APIs.
///
/// Specifies the search query, optional API source filters, and pagination options.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMusicApis {
    /// The search query string
    query: String,
    /// Comma-separated list of API sources to search (searches all if not specified)
    api_source: Option<String>,
    /// Starting offset for pagination
    offset: Option<u32>,
    /// Maximum number of results to return
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["MusicApi"],
        get,
        path = "/search",
        description = "Search the music APIs",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("query" = String, Query, description = "The search query"),
            ("apiSource" = Option<String>, Query, description = "The ApiSource(s) to search"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of search results for the music APIs",
                body = Value,
            )
        )
    )
)]
#[route("/search", method = "GET")]
/// Searches across one or more music APIs for content matching the query.
///
/// Results are returned grouped by API source. If no API sources are specified,
/// all enabled APIs that support search will be queried.
///
/// # Errors
///
/// * Returns a 404 error if the specified profile is not found
/// * Returns a 400 error if invalid API sources are provided
/// * Returns a 500 error if any search operation fails
pub async fn search_music_apis_endpoint(
    query: web::Query<SearchMusicApis>,
    profile_name: ProfileName,
) -> Result<Json<BTreeMap<ApiSource, ApiSearchResultsResponse>>> {
    let api_sources = query
        .api_source
        .as_ref()
        .map(|x| {
            x.split(',')
                .map(std::convert::TryInto::try_into)
                .collect::<Result<Vec<ApiSource>, _>>()
                .map_err(|e| ErrorBadRequest(format!("Invalid apiSource: {e:?}")))
        })
        .transpose()?;
    let profile_name: String = profile_name.into();
    let music_apis = moosicbox_music_api::profiles::PROFILES
        .get(&profile_name)
        .ok_or_else(|| ErrorNotFound(format!("Missing profile '{profile_name}'")))?;
    let music_apis = music_apis
        .iter()
        .filter(|x| x.supports_search())
        .filter(|x| {
            api_sources
                .as_ref()
                .is_none_or(|sources| sources.contains(x.source()))
        })
        .collect::<Vec<_>>();

    let search_results = music_apis
        .iter()
        .map(|x| x.search(&query.query, query.offset, query.limit));
    let search_results = futures::future::join_all(search_results).await;
    let search_results = search_results
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(ErrorInternalServerError)?;

    let search_results = music_apis
        .into_iter()
        .map(|x| x.source().clone())
        .zip(search_results.into_iter())
        .collect::<BTreeMap<_, _>>();

    Ok(Json(search_results))
}
