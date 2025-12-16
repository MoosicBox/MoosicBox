//! Server information endpoints for the admin interface.
//!
//! Provides endpoints for displaying `MoosicBox` server identity and configuration.

use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
};
use maud::{Markup, html};
use switchy_database::{DatabaseError, config::ConfigDatabase};

/// Binds server info endpoints to the provided Actix web scope.
#[must_use]
pub const fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
}

/// Renders server information including the server identity.
///
/// # Errors
///
/// * If fails to get the server identity from the database
pub async fn info(db: &ConfigDatabase) -> Result<Markup, DatabaseError> {
    let id = moosicbox_config::get_server_identity(db).await?;
    let id = id.unwrap_or_else(|| "(not set)".to_string());

    Ok(html! {
        table {
            tbody {
                tr {
                    td { "Server ID:" } td { (id) }
                }
            }
        }
    })
}
