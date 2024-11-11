use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    route,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::api::models::ApiAudioOutput;

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(audio_outputs_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Audio Output")),
    paths(audio_outputs_endpoint),
    components(schemas())
)]
pub struct Api;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioOutputs {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Output"],
        get,
        path = "/audio-outputs",
        description = "Get a list of the enabled audio outputs",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of audio outputs",
                body = Value,
            )
        )
    )
)]
#[route("/audio-outputs", method = "GET")]
pub async fn audio_outputs_endpoint(
    query: web::Query<GetAudioOutputs>,
) -> Result<Json<Page<ApiAudioOutput>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let outputs = crate::output_factories().await;
    let total = outputs.len() as u32;
    let outputs = outputs
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.into())
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: outputs,
        offset,
        limit,
        total,
    }))
}
