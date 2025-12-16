//! HTTP API endpoints for managing audio outputs.
//!
//! This module provides REST API endpoints for querying available audio output devices
//! and their configurations. The API is built on Actix-web and includes `OpenAPI` documentation.

#![allow(clippy::needless_for_each)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    route,
    web::{self, Json},
};
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::api::models::ApiAudioOutput;

/// Data models for audio output API responses.
pub mod models;

/// Binds audio output API endpoints to an Actix-web scope.
///
/// This function registers all audio output-related HTTP endpoints with the provided scope.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(audio_outputs_endpoint)
}

/// `OpenAPI` documentation for the audio output API.
///
/// Provides schema definitions and endpoint documentation for the audio output API.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Audio Output")),
    paths(audio_outputs_endpoint),
    components(schemas())
)]
pub struct Api;

/// Query parameters for retrieving audio outputs.
///
/// Used for paginated requests to the audio outputs endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioOutputs {
    /// Page offset for pagination.
    offset: Option<u32>,
    /// Maximum number of items to return.
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
/// HTTP endpoint for retrieving available audio outputs.
///
/// Returns a paginated list of available audio output devices.
///
/// # Errors
///
/// * If pagination parameters are invalid
pub async fn audio_outputs_endpoint(
    query: web::Query<GetAudioOutputs>,
) -> Result<Json<Page<ApiAudioOutput>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let outputs = crate::output_factories().await;
    let total = u32::try_from(outputs.len()).unwrap();
    let outputs = outputs
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: outputs,
        offset,
        limit,
        total,
    }))
}
