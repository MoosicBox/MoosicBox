use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result,
};
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::api::models::ApiAudioZone;

pub mod models;

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Audio Zone")),
    paths(audio_zones_endpoint),
    components(schemas())
)]
pub struct Api;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioZones {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        get,
        path = "/audio-zones",
        description = "Get a list of the enabled audio zones",
        params(
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of audio zones",
                body = Value,
            )
        )
    )
)]
#[route("/audio-zones", method = "GET")]
pub async fn audio_zones_endpoint(
    query: web::Query<GetAudioZones>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAudioZone>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let zones = crate::zones(&**data.database)
        .await
        .map_err(ErrorInternalServerError)?;
    let total = zones.len() as u32;
    let zones = zones
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.into())
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: zones,
        offset,
        limit,
        total,
    }))
}
