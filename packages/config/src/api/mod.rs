//! HTTP API endpoints for `MoosicBox` configuration.
//!
//! This module provides `RESTful` API endpoints for managing `MoosicBox` profiles
//! and configuration through HTTP requests.
//!
//! # Endpoints
//!
//! * `GET /profiles` - Retrieve all profiles
//! * `POST /profiles?name=<name>` - Create or retrieve a profile by name
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "api")]
//! # {
//! use actix_web::App;
//! use moosicbox_config::api::bind_services;
//!
//! let app = App::new()
//!     .service(bind_services(actix_web::web::scope("/config")));
//! # }
//! ```

#![allow(clippy::needless_for_each)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
};
use serde::Deserialize;
use switchy_database::config::ConfigDatabase;

use crate::api::models::ApiProfile;

pub mod models;

/// Binds the configuration API endpoints to an Actix-Web scope.
///
/// This function registers the profiles endpoints with the provided scope.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(get_profiles_endpoint)
        .service(create_profile_endpoint)
}

/// `OpenAPI` documentation structure for the configuration API.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Config")),
    paths(
        get_profiles_endpoint,
        create_profile_endpoint,
    ),
    components(schemas())
)]
pub struct Api;

/// API endpoint to retrieve all `MoosicBox` profiles.
///
/// # Errors
///
/// * If a database error occurs while retrieving profiles
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Config"],
        get,
        path = "/profiles",
        description = "Get list of MoosicBox profiles",
        params(),
        responses(
            (
                status = 200,
                description = "The list of MoosicBox profiles",
                body = Vec<ApiProfile>,
            )
        )
    )
)]
#[route("/profiles", method = "GET")]
pub async fn get_profiles_endpoint(db: ConfigDatabase) -> Result<Json<Vec<ApiProfile>>> {
    Ok(Json(
        crate::db::get_profiles(&db)
            .await
            .map_err(ErrorInternalServerError)?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiProfile>>(),
    ))
}

/// Query parameters for creating a profile.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileQuery {
    /// Name of the profile to create or retrieve
    pub name: String,
}

/// API endpoint to create a new `MoosicBox` profile.
///
/// If a profile with the given name already exists, returns the existing profile.
///
/// # Errors
///
/// * If a database error occurs while creating the profile
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Config"],
        post,
        path = "/profiles",
        description = "Create a new MoosicBox profile",
        params(
            ("name" = String, Query, description = "The name of the profile"),
        ),
        responses(
            (
                status = 200,
                description = "The created MoosicBox profile",
                body = ApiProfile,
            )
        )
    )
)]
#[route("/profiles", method = "POST")]
pub async fn create_profile_endpoint(
    query: web::Query<CreateProfileQuery>,
    db: ConfigDatabase,
) -> Result<Json<ApiProfile>> {
    Ok(Json(
        crate::db::upsert_profile(&db, &query.name)
            .await
            .map_err(ErrorInternalServerError)?
            .into(),
    ))
}
