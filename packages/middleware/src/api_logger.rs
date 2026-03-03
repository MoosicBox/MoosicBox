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

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::{App, HttpResponse, http::StatusCode, test, web};

    async fn success_handler() -> HttpResponse {
        HttpResponse::Ok().body("success")
    }

    async fn created_handler() -> HttpResponse {
        HttpResponse::Created().body("created")
    }

    async fn internal_server_error_handler() -> HttpResponse {
        HttpResponse::InternalServerError().body("internal server error")
    }

    async fn redirect_handler() -> HttpResponse {
        HttpResponse::Found()
            .insert_header(("Location", "http://example.com"))
            .finish()
    }

    async fn informational_handler() -> HttpResponse {
        HttpResponse::Continue().finish()
    }

    async fn bad_request_handler() -> HttpResponse {
        HttpResponse::BadRequest().body("bad request")
    }

    async fn not_found_handler() -> HttpResponse {
        HttpResponse::NotFound().body("not found")
    }

    async fn range_response_handler() -> HttpResponse {
        HttpResponse::PartialContent()
            .insert_header((header::CONTENT_RANGE, "bytes 0-99/1000"))
            .insert_header((header::ACCEPT_RANGES, "bytes"))
            .insert_header((header::CONTENT_LENGTH, "100"))
            .body("partial content")
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_success_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/", web::get().to(success_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_created_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/resource", web::post().to(created_handler)),
        )
        .await;

        let req = test::TestRequest::post().uri("/resource").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_redirect_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/redirect", web::get().to(redirect_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/redirect").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::FOUND);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_informational_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/continue", web::get().to(informational_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/continue").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::CONTINUE);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_client_error_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/bad", web::get().to(bad_request_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/bad").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_passes_through_not_found_response() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/missing", web::get().to(not_found_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/missing").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_handles_request_with_query_string() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/search", web::get().to(success_handler)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/search?q=test&page=1")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_handles_request_with_range_header() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/data", web::get().to(range_response_handler)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/data")
            .insert_header((header::RANGE, "bytes=0-99"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_handles_response_with_content_range_headers() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/partial", web::get().to(range_response_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/partial").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert!(resp.headers().contains_key(header::CONTENT_RANGE));
        assert!(resp.headers().contains_key(header::ACCEPT_RANGES));
        assert!(resp.headers().contains_key(header::CONTENT_LENGTH));
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_handles_different_http_methods() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/resource", web::post().to(created_handler))
                .route("/resource", web::get().to(success_handler)),
        )
        .await;

        let get_req = test::TestRequest::get().uri("/resource").to_request();
        let get_resp = test::call_service(&app, get_req).await;
        assert_eq!(get_resp.status(), StatusCode::OK);

        let post_req = test::TestRequest::post().uri("/resource").to_request();
        let post_resp = test::call_service(&app, post_req).await;
        assert_eq!(post_resp.status(), StatusCode::CREATED);
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_preserves_response_body() {
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/", web::get().to(success_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;

        let body = test::read_body(resp).await;
        assert_eq!(body, "success");
    }

    #[test_log::test]
    fn test_api_logger_default_is_equivalent_to_new() {
        // Both should create the same middleware
        let _from_new = ApiLogger::new();
        let _from_default = ApiLogger::default();
        // If this compiles and runs, both constructors work correctly
    }

    #[test_log::test(actix_web::test)]
    async fn test_middleware_handles_server_error_response() {
        // Tests the specific code path where is_server_error() returns true,
        // which triggers error logging and the assertion check.
        // This is distinct from client errors (4xx) which don't trigger is_server_error().
        let app = test::init_service(
            App::new()
                .wrap(ApiLogger::new())
                .route("/error", web::get().to(internal_server_error_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/error").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
