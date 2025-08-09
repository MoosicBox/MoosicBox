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

#[allow(clippy::module_name_repetitions)]
pub struct StaticTokenAuth {
    token: String,
}

impl StaticTokenAuth {
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
