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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMusicApis {
    offset: Option<u32>,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMusicApi {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanMusicApi {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanMusicApi {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMusicApis {
    query: String,
    api_source: Option<String>,
    offset: Option<u32>,
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
pub async fn search_music_apis_endpoint(
    query: web::Query<SearchMusicApis>,
    profile_name: ProfileName,
) -> Result<Json<BTreeMap<ApiSource, ApiSearchResultsResponse>>> {
    let api_sources = query
        .api_source
        .as_ref()
        .map(|x| {
            x.split(',')
                .map(|x| x.try_into())
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
