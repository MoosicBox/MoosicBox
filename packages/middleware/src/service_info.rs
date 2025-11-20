//! Service configuration information accessible via Actix-web request extraction.
//!
//! This module provides [`ServiceInfo`] which can be extracted in request handlers
//! to access service configuration like the port number.
//!
//! The configuration must be initialized once using [`init`] before the server starts.
//!
//! # Example
//!
//! ```rust
//! use actix_web::{web, App, HttpResponse, HttpServer};
//! use moosicbox_middleware::service_info::ServiceInfo;
//!
//! async fn handler(info: ServiceInfo) -> HttpResponse {
//!     HttpResponse::Ok().body(format!("Running on port {}", info.port))
//! }
//!
//! # async fn example() -> std::io::Result<()> {
//! // Initialize before starting server
//! moosicbox_middleware::service_info::init(ServiceInfo { port: 8080 })
//!     .expect("Failed to initialize service info");
//!
//! HttpServer::new(|| {
//!     App::new()
//!         .route("/", web::get().to(handler))
//! })
//! .bind(("127.0.0.1", 8080))?
//! .run()
//! # ;
//! # Ok(())
//! # }
//! ```

use std::sync::OnceLock;

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
use futures::future::{Ready, err, ok};

static SERVICE_INFO: OnceLock<ServiceInfo> = OnceLock::new();

/// Initializes the global service configuration.
///
/// This must be called once before starting the server, and before any request handlers
/// attempt to extract [`ServiceInfo`].
///
/// # Errors
///
/// * Returns `Err(ServiceInfo)` if `SERVICE_INFO` has already been initialized
pub fn init(service_info: ServiceInfo) -> Result<(), ServiceInfo> {
    SERVICE_INFO.set(service_info)
}

/// Service configuration information accessible via Actix-web request extraction.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    /// The port on which the service is running.
    pub port: u16,
}

impl FromRequest for ServiceInfo {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    /// Extracts service info from the request context.
    ///
    /// # Errors
    ///
    /// * Returns `ErrorInternalServerError` if service info has not been initialized via [`init`]
    fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let Some(service_info) = SERVICE_INFO.get().cloned() else {
            return err(ErrorInternalServerError(
                "Config service_info not initialized",
            ));
        };

        ok(service_info)
    }
}
