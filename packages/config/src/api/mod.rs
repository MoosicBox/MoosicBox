use std::ops::Deref as _;

use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route,
    web::Json,
    Result, Scope,
};
use moosicbox_database::config::ConfigDatabase;

use crate::api::models::ApiProfile;

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(get_profiles_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Config")),
    paths(
        get_profiles_endpoint,
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
                body = Value,
            )
        )
    )
)]
#[route("/profiles", method = "GET")]
pub async fn get_profiles_endpoint(db: ConfigDatabase) -> Result<Json<Vec<ApiProfile>>> {
    Ok(Json(
        crate::db::get_profiles(db.deref())
            .await
            .map_err(ErrorInternalServerError)?
            .into_iter()
            .map(|x| x.into())
            .collect::<Vec<ApiProfile>>(),
    ))
}
