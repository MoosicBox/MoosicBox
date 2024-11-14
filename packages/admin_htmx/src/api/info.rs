use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::config::ConfigDatabase;

pub const fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
}

/// # Errors
///
/// * If fails to get the server identity from the database
pub async fn info(db: &ConfigDatabase) -> Result<Markup, DbError> {
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
