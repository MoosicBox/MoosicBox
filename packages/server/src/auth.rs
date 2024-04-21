use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorUnauthorized,
    http,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

pub struct StaticTokenAuth {
    token: String,
}

impl StaticTokenAuth {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

// Middleware factory is `Transform` trait
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for StaticTokenAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = StaticTokenAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(StaticTokenAuthMiddleware {
            service,
            token: self.token.clone(),
        }))
    }
}

pub struct StaticTokenAuthMiddleware<S> {
    service: S,
    token: String,
}

impl<S, B> Service<ServiceRequest> for StaticTokenAuthMiddleware<S>
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
        if req.path() == "/health" || req.method() == http::Method::OPTIONS {
            return Box::pin(self.service.call(req));
        }

        if let Some(auth) = req.headers().get(http::header::AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                let token = if auth.to_lowercase().starts_with("bearer") {
                    auth[6..].trim_start()
                } else {
                    auth
                };

                if token == self.token {
                    return Box::pin(self.service.call(req));
                }
            }
        }

        log::warn!(
            "Unauthorized StaticTokenAuthMiddleware request to '{}'",
            req.path()
        );
        Box::pin(async move { Err(ErrorUnauthorized("Unauthorized")) })
    }
}
