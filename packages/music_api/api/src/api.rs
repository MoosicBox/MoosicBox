use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorNotFound,
    route,
    web::{self, Json},
};
use moosicbox_paging::Page;
use moosicbox_profiles::api::ProfileName;
use serde::Deserialize;

use crate::models::ApiMusicApi;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(music_apis_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "MusicApi")),
    paths(
        music_apis_endpoint,
    ),
    components(schemas())
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
        description = "Get a list of tracks associated with a music APIs",
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
    let music_apis = music_apis
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect();

    Ok(Json(Page::WithTotal {
        items: music_apis,
        offset,
        limit,
        total,
    }))
}
