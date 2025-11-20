//! Tunnel configuration information accessible via Actix-web request extraction.
//!
//! This module provides [`TunnelInfo`] which can be extracted in request handlers
//! to access tunnel configuration such as the tunnel host.
//!
//! The configuration must be initialized once using [`init`] before the server starts.
//!
//! # Example
//!
//! ```rust
//! use actix_web::{web, App, HttpResponse, HttpServer};
//! use moosicbox_middleware::tunnel_info::TunnelInfo;
//! use std::sync::Arc;
//!
//! async fn handler(info: TunnelInfo) -> HttpResponse {
//!     match info.host.as_ref().as_ref() {
//!         Some(host) => HttpResponse::Ok().body(format!("Tunnel host: {}", host)),
//!         None => HttpResponse::Ok().body("No tunnel configured"),
//!     }
//! }
//!
//! # #[cfg(feature = "tunnel")]
//! # async fn example() -> std::io::Result<()> {
//! // Initialize before starting server
//! moosicbox_middleware::tunnel_info::init(TunnelInfo {
//!     host: Arc::new(Some("tunnel.example.com".to_string()))
//! })
//! .expect("Failed to initialize tunnel info");
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

use std::sync::{Arc, OnceLock};

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
use futures::future::{Ready, err, ok};

static TUNNEL_INFO: OnceLock<TunnelInfo> = OnceLock::new();

/// Initializes the global tunnel configuration.
///
/// This must be called once before starting the server, and before any request handlers
/// attempt to extract [`TunnelInfo`].
///
/// # Errors
///
/// * Returns `Err(TunnelInfo)` if `TUNNEL_INFO` has already been initialized
pub fn init(tunnel_info: TunnelInfo) -> Result<(), TunnelInfo> {
    TUNNEL_INFO.set(tunnel_info)
}

/// Tunnel configuration information accessible via Actix-web request extraction.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct TunnelInfo {
    /// The tunnel host, if configured.
    pub host: Arc<Option<String>>,
}

impl FromRequest for TunnelInfo {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    /// Extracts tunnel info from the request context.
    ///
    /// # Errors
    ///
    /// * Returns `ErrorInternalServerError` if tunnel info has not been initialized via [`init`]
    fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let Some(tunnel_info) = TUNNEL_INFO.get().cloned() else {
            return err(ErrorInternalServerError(
                "Config tunnel_info not initialized",
            ));
        };

        ok(tunnel_info)
    }
}
