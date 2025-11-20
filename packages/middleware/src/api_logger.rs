//! Actix-web middleware for logging HTTP request and response details.
//!
//! This module provides [`ApiLogger`] middleware that logs:
//! * Request method, path, query string, and relevant headers (e.g., Range)
//! * Response status, duration, and relevant headers (e.g., Content-Range, Content-Length)
//! * Different log levels for success (trace) vs. failure (error) responses
//!
//! # Example
//!
//! ```rust
//! use actix_web::{App, HttpServer};
//! use moosicbox_middleware::api_logger::ApiLogger;
//!
//! # async fn example() -> std::io::Result<()> {
//! HttpServer::new(|| {
//!     App::new()
//!         .wrap(ApiLogger::new())
//!         // ... add your routes
//! })
//! .bind(("127.0.0.1", 8080))?
//! .run()
//! # ;
//! # Ok(())
//! # }
//! ```

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header,
};
use futures_util::{FutureExt, future::LocalBoxFuture};
use std::future::{Ready, ready};

/// Actix-web middleware for logging API requests and responses.
///
/// Logs request details (method, path, query, headers) and response details
/// (status, duration, headers) at appropriate log levels.
#[allow(clippy::module_name_repetitions)]
pub struct ApiLogger {}

impl ApiLogger {
    /// Creates a new API logger middleware.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for ApiLogger {
    /// Creates a default API logger instance.
    fn default() -> Self {
        Self::new()
    }
}

// Middleware factory is `Transform` trait
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for ApiLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = ApiLoggerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    /// Creates a new middleware instance wrapping the given service.
    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ApiLoggerMiddleware { service }))
    }
}

/// The actual middleware service that wraps the next service in the chain.
///
/// This struct is created by the [`Transform`] implementation on [`ApiLogger`]
/// and performs the actual request/response logging.
#[allow(clippy::module_name_repetitions)]
pub struct ApiLoggerMiddleware<S> {
    /// The next service in the middleware chain.
    service: S,
}

impl<S, B> Service<ServiceRequest> for ApiLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    /// Processes the request, logs details, and logs the response when complete.
    ///
    /// Logs request method, path, query string, and relevant headers (e.g., Range).
    /// After the response completes, logs status, duration, and relevant response headers
    /// (e.g., Content-Range, Content-Length).
    ///
    /// Successful responses are logged at trace level, while failures are logged at error level.
    fn call(&self, req: ServiceRequest) -> Self::Future {
        const RELEVANT_HEADER_NAMES: [header::HeaderName; 1] = [header::RANGE];
        let relevant_headers = req
            .headers()
            .iter()
            .filter(|(name, _)| RELEVANT_HEADER_NAMES.iter().any(|x| x == name))
            .collect::<Vec<_>>();
        let prefix = format!(
            "{method} {path}{query} headers={headers:?}",
            method = req.method(),
            path = req.path(),
            query = if req.query_string().is_empty() {
                String::new()
            } else {
                format!("?{}", req.query_string())
            },
            headers = relevant_headers,
        );
        let start = switchy_time::instant_now();
        log::trace!("{prefix} STARTED");
        Box::pin(self.service.call(req).then(move |response| async move {
            let duration = switchy_time::instant_now()
                .duration_since(start)
                .as_millis();
            match response {
                Ok(data) => {
                    const RELEVANT_HEADER_NAMES: [header::HeaderName; 3] = [
                        header::CONTENT_RANGE,
                        header::ACCEPT_RANGES,
                        header::CONTENT_LENGTH,
                    ];
                    let relevant_headers = data
                        .response()
                        .headers()
                        .iter()
                        .filter(|(name, _)| RELEVANT_HEADER_NAMES.iter().any(|x| x == name))
                        .collect::<Vec<_>>();
                    let prefix = format!("{prefix} resp_headers={relevant_headers:?}");
                    let status = data.response().status();
                    if status.is_success() || status.is_redirection() || status.is_informational() {
                        log::trace!("{prefix} FINISHED SUCCESS \"{status}\" ({duration} ms)");
                    } else {
                        let e = data.response().error();
                        let error_message = e.map_or_else(String::new, |e| format!(": {e:?}"));

                        log::error!(
                            "{prefix} FINISHED FAILURE \"{status}\" ({duration} ms){error_message}"
                        );

                        tracing::error!(
                            name: "FINISHED FAILURE",
                            name = "FINISHED FAILURE",
                            status = status.to_string(),
                            duration = duration.to_string(),
                            error = format!("{:?}", e)
                        );

                        moosicbox_assert::assert!(!status.is_server_error());
                    }
                    Ok(data)
                }
                Err(e) => {
                    moosicbox_assert::die_or_error!(
                        "{prefix} FINISHED ERROR ({duration} ms): {e:?}"
                    );
                    Err(e)
                }
            }
        }))
    }
}
