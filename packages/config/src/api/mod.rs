use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
};
use moosicbox_database::config::ConfigDatabase;
use serde::Deserialize;

use crate::api::models::ApiProfile;

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(get_profiles_endpoint)
        .service(create_profile_endpoint)
}

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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileQuery {
    name: String,
}

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
