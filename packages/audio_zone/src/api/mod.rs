//! HTTP API endpoints for audio zone management.
//!
//! This module provides Actix-web route handlers for creating, reading, updating, and deleting
//! audio zones via REST API. All endpoints support pagination and return JSON responses.
//!
//! # Endpoints
//!
//! * `GET /` - List audio zones with pagination
//! * `GET /with-session` - List audio zones with their active playback sessions
//! * `POST /` - Create a new audio zone
//! * `PATCH /` - Update an existing audio zone
//! * `DELETE /` - Delete an audio zone
//!
//! # Examples
//!
//! ```rust,no_run
//! use actix_web::{App, web};
//! use moosicbox_audio_zone::api;
//!
//! # fn main() {
//! let app = App::new()
//!     .service(
//!         api::bind_services(web::scope("/audio-zones"))
//!     );
//! # }
//! ```

#![allow(clippy::needless_for_each)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
};
use moosicbox_paging::Page;
use serde::Deserialize;
use switchy_database::{config::ConfigDatabase, profiles::LibraryDatabase};

use crate::models::{ApiAudioZone, ApiAudioZoneWithSession, CreateAudioZone, UpdateAudioZone};

pub mod models;

/// Binds all audio zone API endpoints to the provided Actix-web scope.
///
/// This registers the following HTTP endpoints:
/// * GET `/` - List audio zones
/// * GET `/with-session` - List audio zones with their sessions
/// * POST `/` - Create a new audio zone
/// * PATCH `/` - Update an existing audio zone
/// * DELETE `/` - Delete an audio zone
#[must_use]
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

/// `OpenAPI` specification for audio zone endpoints.
///
/// This struct provides the `OpenAPI` documentation for all audio zone API endpoints
/// when the `openapi` feature is enabled.
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

/// Query parameters for listing audio zones.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioZones {
    /// The number of items to skip (for pagination).
    offset: Option<u32>,
    /// The maximum number of items to return (for pagination).
    limit: Option<u32>,
}

/// HTTP endpoint for retrieving a paginated list of audio zones.
///
/// Returns all configured audio zones with support for pagination via `offset` and `limit`
/// query parameters.
///
/// # Errors
///
/// * If there is a database error while fetching audio zones
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
    #[allow(clippy::cast_possible_truncation)]
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

/// Query parameters for listing audio zones with their sessions.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioZoneWithSessions {
    /// The number of items to skip (for pagination).
    offset: Option<u32>,
    /// The maximum number of items to return (for pagination).
    limit: Option<u32>,
}

/// HTTP endpoint for retrieving audio zones along with their active playback sessions.
///
/// Returns a paginated list of audio zones that currently have active playback sessions,
/// combining data from both the configuration and library databases.
///
/// # Errors
///
/// * If there is a database error while fetching audio zones or sessions
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
    #[allow(clippy::cast_possible_truncation)]
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

/// Query parameters for creating a new audio zone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZoneQuery {
    /// The name of the audio zone to create.
    pub name: String,
}

/// HTTP endpoint for creating a new audio zone.
///
/// Creates a new audio zone with the specified name from the query parameters.
/// If the `events` feature is enabled, triggers an audio zones updated event after creation.
///
/// # Errors
///
/// * If there is a database error while creating the audio zone
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

/// Query parameters for deleting an audio zone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAudioZoneQuery {
    /// The ID of the audio zone to delete.
    pub id: u64,
}

/// HTTP endpoint for deleting an audio zone.
///
/// Deletes the audio zone with the specified ID from the query parameters.
/// If the `events` feature is enabled, triggers an audio zones updated event after deletion.
///
/// # Errors
///
/// * If there is a database error while deleting the audio zone
/// * If no audio zone with the specified ID exists (returns HTTP 404)
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
        .ok_or_else(|| ErrorNotFound("Audio zone not found"))?
        .into();

    Ok(Json(zone))
}

/// Query parameters for updating an audio zone.
///
/// The actual update data is provided in the request body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAudioZoneQuery {}

/// HTTP endpoint for updating an existing audio zone.
///
/// Updates an audio zone's properties based on the data provided in the request body.
/// If the `events` feature is enabled, triggers an audio zones updated event after the update.
///
/// # Errors
///
/// * If there is a database error while updating the audio zone
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
