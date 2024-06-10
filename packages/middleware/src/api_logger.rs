use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

#[allow(clippy::module_name_repetitions)]
pub struct ApiLogger {}

impl ApiLogger {
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for ApiLogger {
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

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ApiLoggerMiddleware { service }))
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct ApiLoggerMiddleware<S> {
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

    fn call(&self, req: ServiceRequest) -> Self::Future {
        log::trace!(
            "{} {}{}",
            req.method(),
            req.path(),
            if req.query_string().is_empty() {
                String::new()
            } else {
                format!("?{}", req.query_string())
            }
        );
        Box::pin(self.service.call(req))
    }
}
