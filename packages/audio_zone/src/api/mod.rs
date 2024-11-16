use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_database::{config::ConfigDatabase, profiles::LibraryDatabase};
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::models::{ApiAudioZone, ApiAudioZoneWithSession, CreateAudioZone, UpdateAudioZone};

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(audio_zones_endpoint)
        .service(audio_zone_with_sessions_endpoint)
        .service(create_audio_zone_endpoint)
        .service(update_audio_zone_endpoint)
        .service(delete_audio_zone_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Audio Zone")),
    paths(
        audio_zones_endpoint,
        audio_zone_with_sessions_endpoint,
        create_audio_zone_endpoint,
        update_audio_zone_endpoint,
        delete_audio_zone_endpoint,
    ),
    components(schemas(
        ApiAudioZone,
        UpdateAudioZone,
        crate::models::ApiPlayer,
    ))
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
        path = "",
        description = "Get a list of the enabled audio zones",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
#[route("", method = "GET")]
pub async fn audio_zones_endpoint(
    query: web::Query<GetAudioZones>,
    db: ConfigDatabase,
) -> Result<Json<Page<ApiAudioZone>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let zones = crate::zones(&db).await.map_err(ErrorInternalServerError)?;
    let total = zones.len() as u32;
    let zones = zones
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: zones,
        offset,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioZoneWithSessions {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        get,
        path = "/with-session",
        description = "Get a list of the enabled audio zones with their corresponding session",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of audio zones with their corresponding session",
                body = Value,
            )
        )
    )
)]
#[route("/with-session", method = "GET")]
pub async fn audio_zone_with_sessions_endpoint(
    query: web::Query<GetAudioZoneWithSessions>,
    config_db: ConfigDatabase,
    library_db: LibraryDatabase,
) -> Result<Json<Page<ApiAudioZoneWithSession>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let zones = crate::zones_with_sessions(&config_db, &library_db)
        .await
        .map_err(ErrorInternalServerError)?;
    let total = zones.len() as u32;
    let zones = zones
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: zones,
        offset,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZoneQuery {
    pub name: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        post,
        path = "",
        description = "Create a new audio zone",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("name" = String, Query, description = "Name of the audio zone to create"),
        ),
        responses(
            (
                status = 200,
                description = "The audio zone that was successfully created",
                body = ApiAudioZone,
            )
        )
    )
)]
#[route("", method = "POST")]
pub async fn create_audio_zone_endpoint(
    query: web::Query<CreateAudioZoneQuery>,
    db: ConfigDatabase,
) -> Result<Json<ApiAudioZone>> {
    let create = CreateAudioZone {
        name: query.name.clone(),
    };
    let zone = crate::create_audio_zone(&db, &create)
        .await
        .map_err(ErrorInternalServerError)?
        .into();

    Ok(Json(zone))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAudioZoneQuery {
    pub id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        post,
        path = "",
        description = "Delete a new audio zone",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("name" = String, Query, description = "Name of the audio zone to delete"),
        ),
        responses(
            (
                status = 200,
                description = "The audio zone that was successfully deleted",
                body = ApiAudioZone,
            )
        )
    )
)]
#[route("", method = "DELETE")]
pub async fn delete_audio_zone_endpoint(
    query: web::Query<DeleteAudioZoneQuery>,
    db: ConfigDatabase,
) -> Result<Json<ApiAudioZone>> {
    let zone = crate::delete_audio_zone(&db, query.id)
        .await
        .map_err(ErrorInternalServerError)?
        .ok_or(ErrorNotFound("Audio zone not found"))?
        .into();

    Ok(Json(zone))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAudioZoneQuery {}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        patch,
        path = "",
        request_body = UpdateAudioZone,
        description = "Update an existing audio zone",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
        ),
        responses(
            (
                status = 200,
                description = "The audio zone that was successfully updated",
                body = ApiAudioZone,
            )
        )
    )
)]
#[route("", method = "PATCH")]
pub async fn update_audio_zone_endpoint(
    update: Json<UpdateAudioZone>,
    _query: web::Query<UpdateAudioZoneQuery>,
    db: ConfigDatabase,
) -> Result<Json<ApiAudioZone>> {
    let zone = crate::update_audio_zone(&db, update.clone())
        .await
        .map_err(ErrorInternalServerError)?
        .into();

    Ok(Json(zone))
}
