//! Opt-in HTTP request bounds and sensitive-response protection.

use actix_web::{
    HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::header::{CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, HeaderName, HeaderValue},
    middleware::Next,
    web,
};

/// Opt-in server-wide request bounds and sensitive path response protection.
///
/// Selecting a policy disables built-in request/query/body logging for all requests.
/// Bodies are bounded and buffered before routing, including static and streaming
/// endpoints. This is intended for finite request bodies, not streaming uploads.
/// Application processors and externally installed logging remain responsible for
/// their own secret handling.
#[derive(Clone, Debug)]
pub struct HttpPolicy {
    max_body_bytes: usize,
    sensitive_path_prefixes: Vec<String>,
}

impl HttpPolicy {
    /// Creates a policy with a server-wide body limit in bytes (zero permits empty bodies only).
    #[must_use]
    pub const fn new(max_body_bytes: usize) -> Self {
        Self {
            max_body_bytes,
            sensitive_path_prefixes: Vec::new(),
        }
    }

    /// Protects responses whose URI path starts with this literal prefix.
    ///
    /// Matching is case-sensitive, excludes the query, and is not segment-based:
    /// `/private` also matches `/private-other`. Use `/` (or an empty prefix) to
    /// protect every response, including token-bearing application documents.
    #[must_use]
    pub fn with_sensitive_path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.sensitive_path_prefixes.push(prefix.into());
        self
    }

    /// Returns the server-wide maximum request body size.
    #[must_use]
    pub const fn max_body_bytes(&self) -> usize {
        self.max_body_bytes
    }

    fn is_sensitive(&self, path: &str) -> bool {
        self.sensitive_path_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }
}

// This is the outermost application middleware: bounds run before CORS, auth,
// route guards, static files, SSE, and processors. Bytes extraction checks actual
// streamed bytes (not just Content-Length), using the app's PayloadConfig.
// Actix services and payloads are worker-local by design.
#[allow(clippy::future_not_send)]
pub async fn enforce<B: MessageBody + 'static>(
    mut req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, actix_web::Error> {
    let Some(policy) = req.app_data::<web::Data<HttpPolicy>>().cloned() else {
        return next
            .call(req)
            .await
            .map(ServiceResponse::map_into_left_body);
    };
    let sensitive = policy.is_sensitive(req.path());
    let mut response = match req.extract::<web::Bytes>().await {
        Ok(body) => {
            // Bytes extraction decodes content encodings. Do not let downstream
            // extractors decode the restored bytes twice or trust the wire length.
            req.headers_mut().remove(CONTENT_ENCODING);
            req.headers_mut().remove(CONTENT_LENGTH);
            req.set_payload(body.into());
            next.call(req).await?.map_into_left_body()
        }
        Err(error) => req
            .into_response(HttpResponse::build(error.as_response_error().status_code()).finish())
            .map_into_right_body(),
    };
    if sensitive {
        secure_headers(&mut response);
    }
    Ok(response)
}

fn secure_headers<B>(response: &mut ServiceResponse<B>) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use actix_web::{
        App, HttpResponse,
        http::{StatusCode, header},
        middleware, test, web,
    };

    use super::{HttpPolicy, enforce};

    fn protected<B>(response: &actix_web::dev::ServiceResponse<B>) {
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "no-referrer"
        );
    }

    #[actix_web::test]
    async fn body_limit_precedes_routing_even_without_content_length() {
        let calls = Rc::new(Cell::new(0));
        let handler_calls = calls.clone();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(
                    HttpPolicy::new(4).with_sensitive_path_prefix("/private"),
                ))
                .app_data(web::PayloadConfig::new(4))
                .wrap(middleware::from_fn(enforce))
                .route(
                    "/{path:.*}",
                    web::to(move |body: web::Bytes| {
                        handler_calls.set(handler_calls.get() + 1);
                        async move { HttpResponse::Ok().body(body) }
                    }),
                ),
        )
        .await;

        // set_payload supplies actual bytes, but no Content-Length header here.
        for path in ["/private/api", "/static/file", "/events", "/auth"] {
            let mut request = test::TestRequest::post()
                .uri(path)
                .set_payload("12345")
                .to_request();
            request.head_mut().headers.remove(header::CONTENT_LENGTH);
            assert!(!request.head().headers.contains_key(header::CONTENT_LENGTH));
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
            if path.starts_with("/private") {
                protected(&response);
            }
        }
        assert_eq!(calls.get(), 0);
        let request = test::TestRequest::post()
            .uri("/private/api?token=secret")
            .set_payload("1234")
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        protected(&response);
        assert_eq!(test::read_body(response).await, "1234");
        assert_eq!(calls.get(), 1);
    }

    #[actix_web::test]
    async fn sensitive_documents_errors_and_method_rejections_are_protected() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(
                    HttpPolicy::new(16).with_sensitive_path_prefix("/app"),
                ))
                .app_data(web::PayloadConfig::new(16))
                .wrap(middleware::from_fn(enforce))
                .service(web::resource("/app").route(web::get().to(|| async {
                    HttpResponse::Ok()
                        .insert_header((header::CACHE_CONTROL, "public"))
                        .finish()
                })))
                .route(
                    "/app/error",
                    web::get().to(|| async {
                        Err::<HttpResponse, _>(actix_web::error::ErrorUnauthorized("denied"))
                    }),
                ),
        )
        .await;
        for (request, status) in [
            (
                test::TestRequest::get().uri("/app?token=secret"),
                StatusCode::OK,
            ),
            (
                test::TestRequest::post().uri("/app"),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            (
                test::TestRequest::get().uri("/app/error"),
                StatusCode::UNAUTHORIZED,
            ),
            (
                test::TestRequest::get().uri("/app/missing"),
                StatusCode::NOT_FOUND,
            ),
        ] {
            let response = test::call_service(&app, request.to_request()).await;
            assert_eq!(response.status(), status);
            protected(&response);
        }
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/public?next=/app")
                .to_request(),
        )
        .await;
        assert!(!response.headers().contains_key(header::CACHE_CONTROL));
    }

    #[cfg(feature = "assets")]
    #[actix_web::test]
    #[allow(clippy::literal_string_with_formatting_args)] // Actix route parameter syntax.
    async fn static_asset_media_types_keep_sensitive_response_headers() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(
                    HttpPolicy::new(16).with_sensitive_path_prefix("/assets"),
                ))
                .app_data(web::PayloadConfig::new(16))
                .wrap(middleware::from_fn(enforce))
                .route(
                    "/assets/{file}",
                    web::get().to(|path: web::Path<String>| async move {
                        let extension = std::path::Path::new(path.as_str())
                            .extension()
                            .and_then(|extension| extension.to_str());
                        HttpResponse::Ok()
                            .content_type(crate::static_content_type(extension))
                            .body("asset")
                    }),
                ),
        )
        .await;
        for (file, expected) in [
            ("index.html", "text/html;charset=UTF-8"),
            ("index.htm", "text/html;charset=UTF-8"),
            ("app.wasm", "application/wasm"),
            ("manifest.json", "application/json"),
            ("style.css", "text/css;charset=UTF-8"),
            ("app.js", "text/javascript;charset=UTF-8"),
            ("app.mjs", "text/javascript;charset=UTF-8"),
            ("app.cjs", "text/javascript;charset=UTF-8"),
            ("unknown", "application/octet-stream"),
        ] {
            let response = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&format!("/assets/{file}"))
                    .to_request(),
            )
            .await;
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                expected
            );
            protected(&response);
        }
    }

    #[actix_web::test]
    async fn compressed_bodies_are_bounded_and_restored_without_double_decoding() {
        use std::io::Write as _;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(
                    HttpPolicy::new(64).with_sensitive_path_prefix("/"),
                ))
                .app_data(web::PayloadConfig::new(64))
                .wrap(middleware::from_fn(enforce))
                .route(
                    "/",
                    web::post().to(|body: web::Bytes| async move { HttpResponse::Ok().body(body) }),
                ),
        )
        .await;
        for (body, expected) in [
            ("a".repeat(64), StatusCode::OK),
            ("a".repeat(65), StatusCode::PAYLOAD_TOO_LARGE),
        ] {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(body.as_bytes()).unwrap();
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/")
                    .insert_header((header::CONTENT_ENCODING, "gzip"))
                    .set_payload(encoder.finish().unwrap())
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), expected);
            protected(&response);
            if expected == StatusCode::OK {
                assert_eq!(test::read_body(response).await, body);
            }
        }
    }

    #[actix_web::test]
    async fn no_policy_preserves_normal_routes_and_headers() {
        let app = test::init_service(App::new().wrap(middleware::from_fn(enforce)).route(
            "/normal",
            web::post().to(|body: web::Bytes| async move {
                HttpResponse::Created()
                    .insert_header((header::CACHE_CONTROL, "public"))
                    .body(body)
            }),
        ))
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/normal")
                .set_payload("unchanged")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public"
        );
        assert!(!response.headers().contains_key("referrer-policy"));
        assert_eq!(test::read_body(response).await, "unchanged");
    }
}
