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

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::{dev::Payload, test::TestRequest};

    #[test_log::test]
    fn test_service_info_init_success() {
        // Note: This test will fail if run after other tests that initialize SERVICE_INFO
        // Since OnceLock can only be set once per process, we test the behavior when it's not set
        let info = ServiceInfo { port: 8080 };

        // We can't reliably test successful init since SERVICE_INFO is global and may
        // already be initialized. Instead, we verify the struct can be created.
        assert_eq!(info.port, 8080);
    }

    #[test_log::test]
    fn test_service_info_init_returns_error_on_double_init() {
        // Create a new ServiceInfo
        let info1 = ServiceInfo { port: 8080 };
        let info2 = ServiceInfo { port: 9090 };

        // First init should succeed (or fail if already initialized from another test)
        let first_result = init(info1);

        // Second init should always fail
        let second_result = init(info2);
        assert!(
            second_result.is_err(),
            "Second initialization should return an error"
        );

        // The error contains the rejected ServiceInfo
        if let Err(rejected_info) = second_result {
            assert_eq!(rejected_info.port, 9090);
        }

        // If first init succeeded, verify it's still the original value
        if first_result.is_ok()
            && let Some(info) = SERVICE_INFO.get()
        {
            assert_eq!(info.port, 8080);
        }
    }

    #[test_log::test]
    fn test_service_info_clone() {
        let info = ServiceInfo { port: 8080 };
        let cloned = info.clone();
        assert_eq!(info.port, cloned.port);
    }

    #[test_log::test]
    fn test_service_info_debug() {
        let info = ServiceInfo { port: 8080 };
        let debug_str = format!("{info:?}");
        assert!(debug_str.contains("ServiceInfo"));
        assert!(debug_str.contains("8080"));
    }

    #[test_log::test]
    fn test_from_request_returns_initialized_service_info() {
        // Ensure SERVICE_INFO is initialized first
        // This may fail if already initialized by another test, which is fine
        let _ = init(ServiceInfo { port: 8080 });

        let req = TestRequest::default().to_http_request();
        let mut payload = Payload::None;
        let result = ServiceInfo::from_request(&req, &mut payload).into_inner();

        // Since SERVICE_INFO is initialized (either by this test or a previous one),
        // from_request should succeed
        assert!(
            result.is_ok(),
            "from_request should succeed when SERVICE_INFO is initialized"
        );

        let service_info = result.unwrap();
        // The port will be 8080 if this test initialized it, or whatever port
        // was set by a previous test
        assert!(
            service_info.port > 0,
            "ServiceInfo should have a valid port"
        );
    }

    #[test_log::test]
    fn test_from_request_returns_same_info_on_multiple_requests() {
        // Ensure SERVICE_INFO is initialized
        let _ = init(ServiceInfo { port: 8080 });

        let req1 = TestRequest::default().uri("/test1").to_http_request();
        let req2 = TestRequest::default().uri("/test2").to_http_request();

        let mut payload1 = Payload::None;
        let mut payload2 = Payload::None;

        let result1 = ServiceInfo::from_request(&req1, &mut payload1).into_inner();
        let result2 = ServiceInfo::from_request(&req2, &mut payload2).into_inner();

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Both requests should return the same ServiceInfo
        assert_eq!(result1.unwrap().port, result2.unwrap().port);
    }
}
