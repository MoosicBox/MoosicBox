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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_tunnel_info_with_host() {
        let info = TunnelInfo {
            host: Arc::new(Some("tunnel.example.com".to_string())),
        };

        assert!(info.host.is_some());
        assert_eq!(info.host.as_ref().as_ref().unwrap(), "tunnel.example.com");
    }

    #[test_log::test]
    fn test_tunnel_info_without_host() {
        let info = TunnelInfo {
            host: Arc::new(None),
        };

        assert!(info.host.is_none());
    }

    #[test_log::test]
    fn test_tunnel_info_init_returns_error_on_double_init() {
        // Create two TunnelInfo instances
        let info1 = TunnelInfo {
            host: Arc::new(Some("first.example.com".to_string())),
        };
        let info2 = TunnelInfo {
            host: Arc::new(Some("second.example.com".to_string())),
        };

        // First init should succeed (or fail if already initialized from another test)
        let first_result = init(info1);

        // Second init should always fail
        let second_result = init(info2);
        assert!(
            second_result.is_err(),
            "Second initialization should return an error"
        );

        // The error contains the rejected TunnelInfo
        if let Err(rejected_info) = second_result {
            assert_eq!(
                rejected_info.host.as_ref().as_ref().unwrap(),
                "second.example.com"
            );
        }

        // If first init succeeded, verify it's still the original value
        if first_result.is_ok()
            && let Some(info) = TUNNEL_INFO.get()
        {
            assert_eq!(info.host.as_ref().as_ref().unwrap(), "first.example.com");
        }
    }

    #[test_log::test]
    fn test_tunnel_info_clone() {
        let info = TunnelInfo {
            host: Arc::new(Some("tunnel.example.com".to_string())),
        };
        let cloned = info.clone();

        // Both should point to the same Arc
        assert_eq!(info.host.as_ref(), cloned.host.as_ref());
        assert_eq!(
            Arc::strong_count(&info.host),
            Arc::strong_count(&cloned.host)
        );
    }

    #[test_log::test]
    fn test_tunnel_info_debug() {
        let info = TunnelInfo {
            host: Arc::new(Some("tunnel.example.com".to_string())),
        };
        let debug_str = format!("{info:?}");
        assert!(debug_str.contains("TunnelInfo"));
        assert!(debug_str.contains("tunnel.example.com"));
    }

    #[test_log::test]
    fn test_tunnel_info_arc_sharing() {
        let host = Arc::new(Some("shared.example.com".to_string()));
        let info1 = TunnelInfo { host: host.clone() };
        let info2 = TunnelInfo { host: host.clone() };

        // Verify both share the same Arc
        assert_eq!(Arc::strong_count(&host), 3); // original + info1 + info2
        assert_eq!(info1.host.as_ref(), info2.host.as_ref());
    }
}
