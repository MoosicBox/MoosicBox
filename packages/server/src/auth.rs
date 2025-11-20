//! Static token authentication middleware.
//!
//! This module provides Actix-web middleware for authenticating requests using a static bearer
//! token. It's enabled with the `static-token-auth` feature and validates tokens from either
//! the Authorization header or query parameters.

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    error::ErrorUnauthorized,
    http,
};
use futures_util::future::LocalBoxFuture;
use qstring::QString;
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
};

/// Static token authentication middleware factory.
///
/// This middleware validates requests using a static bearer token. It's enabled with the
/// `static-token-auth` feature and is intended for simple authentication scenarios where
/// a single shared token is sufficient.
///
/// Requests are authenticated via:
/// * `Authorization` header (with or without "Bearer" prefix)
/// * `authorization` query parameter
///
/// Health check and OPTIONS requests bypass authentication.
#[allow(clippy::module_name_repetitions)]
pub struct StaticTokenAuth {
    token: String,
}

impl StaticTokenAuth {
    /// Creates a new static token authentication middleware.
    #[must_use]
    pub const fn new(token: String) -> Self {
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

/// The actual middleware service that performs static token authentication.
///
/// This is created by the [`StaticTokenAuth`] factory and processes individual requests.
pub struct StaticTokenAuthMiddleware<S> {
    service: S,
    token: String,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
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

        if is_header_authorized(&req, &self.token) || is_query_authorized(&req, &self.token) {
            return Box::pin(self.service.call(req));
        }

        log::warn!(
            "Unauthorized StaticTokenAuthMiddleware {} request to '{}'",
            req.method(),
            req.path(),
        );
        Box::pin(async move { Err(ErrorUnauthorized("Unauthorized")) })
    }
}

#[cfg_attr(feature = "profiling", profiling::function)]
fn is_header_authorized(req: &ServiceRequest, expected: &str) -> bool {
    if let Some(auth) = req.headers().get(http::header::AUTHORIZATION) {
        if let Ok(auth) = auth.to_str() {
            let token = if auth.to_lowercase().starts_with("bearer") {
                auth[6..].trim_start()
            } else {
                auth
            };

            if token == expected {
                return true;
            }
            log::debug!("Incorrect AUTHORIZATION header value");
        } else {
            log::debug!("No AUTHORIZATION header value");
        }
    }

    false
}

#[cfg_attr(feature = "profiling", profiling::function)]
fn is_query_authorized(req: &ServiceRequest, expected: &str) -> bool {
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: BTreeMap<_, _> = query.into_iter().collect();
    let authorization = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(http::header::AUTHORIZATION.as_str()))
        .map(|(_, value)| value);

    if let Some(token) = authorization {
        if token == expected {
            return true;
        }
        log::debug!("Incorrect AUTHORIZATION query param value");
    } else {
        log::debug!("No AUTHORIZATION query param value");
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_token_auth_new() {
        let token = "my_secret_token".to_string();
        let auth = StaticTokenAuth::new(token.clone());
        assert_eq!(auth.token, token);
    }
}
