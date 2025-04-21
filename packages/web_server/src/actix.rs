use std::{
    future::{self},
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, RwLock},
};

use actix_http::{Request, Response, header::LOCATION};
use actix_service::{IntoServiceFactory, Service, ServiceFactory, fn_factory};
use actix_web::{
    Error, HttpServer, Resource, Responder,
    body::MessageBody,
    dev::{self, AppConfig, ServerHandle, ServiceRequest, ServiceResponse},
    error::{self},
};
use futures_util::{FutureExt, future::LocalBoxFuture};
use moosicbox_http_models::{StatusCode, TryFromU16StatusCodeError};
use moosicbox_web_server_core::WebServer;
#[cfg(feature = "cors")]
use moosicbox_web_server_cors::AllOrSome;

#[allow(clippy::fallible_impl_from)]
impl From<HttpRequest> for actix_web::HttpRequest {
    fn from(value: HttpRequest) -> Self {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match value {
            HttpRequest::Actix(x) => x,
            _ => panic!("Invalid HttpRequest"),
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl<'a> From<HttpRequestRef<'a>> for &'a actix_web::HttpRequest {
    fn from(value: HttpRequestRef<'a>) -> Self {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match value {
            HttpRequestRef::Actix(x) => x,
            _ => panic!("Invalid HttpRequest"),
        }
    }
}

impl From<actix_web::HttpRequest> for HttpRequest {
    fn from(value: actix_web::HttpRequest) -> Self {
        Self::Actix(value)
    }
}

impl<'a> From<&'a actix_web::HttpRequest> for HttpRequestRef<'a> {
    fn from(value: &'a actix_web::HttpRequest) -> Self {
        Self::Actix(value)
    }
}

impl From<crate::Error> for Error {
    fn from(value: crate::Error) -> Self {
        match value {
            crate::Error::Http {
                status_code,
                source,
            } => match status_code {
                StatusCode::BadRequest => error::ErrorBadRequest(source),
                StatusCode::Unauthorized => error::ErrorUnauthorized(source),
                StatusCode::PaymentRequired => error::ErrorPaymentRequired(source),
                StatusCode::Forbidden => error::ErrorForbidden(source),
                StatusCode::NotFound => error::ErrorNotFound(source),
                StatusCode::MethodNotAllowed => error::ErrorMethodNotAllowed(source),
                StatusCode::NotAcceptable => error::ErrorNotAcceptable(source),
                StatusCode::ProxyAuthenticationRequired => {
                    error::ErrorProxyAuthenticationRequired(source)
                }
                StatusCode::RequestTimeout => error::ErrorRequestTimeout(source),
                StatusCode::Conflict => error::ErrorConflict(source),
                StatusCode::Gone => error::ErrorGone(source),
                StatusCode::LengthRequired => error::ErrorLengthRequired(source),
                StatusCode::PreconditionFailed => error::ErrorPreconditionFailed(source),
                StatusCode::ContentTooLarge => error::ErrorPayloadTooLarge(source),
                StatusCode::URITooLong => error::ErrorUriTooLong(source),
                StatusCode::UnsupportedMediaType => error::ErrorUnsupportedMediaType(source),
                StatusCode::RangeNotSatisfiable => error::ErrorRangeNotSatisfiable(source),
                StatusCode::ExpectationFailed => error::ErrorExpectationFailed(source),
                StatusCode::ImATeapot => error::ErrorImATeapot(source),
                StatusCode::MisdirectedRequest => error::ErrorMisdirectedRequest(source),
                StatusCode::UncompressableContent => error::ErrorUnprocessableEntity(source),
                StatusCode::Locked => error::ErrorLocked(source),
                StatusCode::FailedDependency => error::ErrorFailedDependency(source),
                StatusCode::UpgradeRequired => error::ErrorUpgradeRequired(source),
                StatusCode::PreconditionRequired => error::ErrorPreconditionRequired(source),
                StatusCode::TooManyRequests => error::ErrorTooManyRequests(source),
                StatusCode::RequestHeaderFieldsTooLarge => {
                    error::ErrorRequestHeaderFieldsTooLarge(source)
                }
                StatusCode::UnavailableForLegalReasons => {
                    error::ErrorUnavailableForLegalReasons(source)
                }
                StatusCode::Continue
                | StatusCode::SwitchingProtocols
                | StatusCode::Processing
                | StatusCode::EarlyHints
                | StatusCode::Ok
                | StatusCode::Created
                | StatusCode::Accepted
                | StatusCode::NonAuthoritativeInformation
                | StatusCode::NoContent
                | StatusCode::ResetContent
                | StatusCode::PartialContent
                | StatusCode::MultiStatus
                | StatusCode::AlreadyReported
                | StatusCode::IMUsed
                | StatusCode::MultipleChoices
                | StatusCode::MovedPermanently
                | StatusCode::Found
                | StatusCode::SeeOther
                | StatusCode::NotModified
                | StatusCode::UseProxy
                | StatusCode::TemporaryRedirect
                | StatusCode::PermanentRedirect
                | StatusCode::TooEarly
                | StatusCode::InternalServerError => error::ErrorInternalServerError(source),
                StatusCode::NotImplemented => error::ErrorNotImplemented(source),
                StatusCode::BadGateway => error::ErrorBadGateway(source),
                StatusCode::ServiceUnavailable => error::ErrorServiceUnavailable(source),
                StatusCode::GatewayTimeout => error::ErrorGatewayTimeout(source),
                StatusCode::HTTPVersionNotSupported => error::ErrorHttpVersionNotSupported(source),
                StatusCode::VariantAlsoNegotiates => error::ErrorVariantAlsoNegotiates(source),
                StatusCode::InsufficientStorage => error::ErrorInsufficientStorage(source),
                StatusCode::LoopDetected => error::ErrorLoopDetected(source),
                StatusCode::NotExtended => error::ErrorNotExtended(source),
                StatusCode::NetworkAuthenticationRequired => {
                    error::ErrorNetworkAuthenticationRequired(source)
                }
            },
        }
    }
}

impl TryFrom<Error> for crate::Error {
    type Error = TryFromU16StatusCodeError;

    fn try_from(value: Error) -> Result<Self, Self::Error> {
        Ok(Self::from_http_status_code(
            StatusCode::try_from_u16(value.error_response().status().as_u16())?,
            value,
        ))
    }
}

impl Service<ServiceRequest> for crate::Route {
    type Response = ServiceResponse;
    type Error = crate::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    dev::always_ready!();

    #[allow(clippy::similar_names)]
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let (req, _) = req.into_parts();

        let handler = self.handler;

        Box::pin(async move {
            handler(req.clone().into())
                .map(|x| {
                    x.map(|x| {
                        let mut res = actix_web::HttpResponseBuilder::new(x.status_code.into());
                        if let Some(location) = x.location {
                            res.insert_header((LOCATION, location));
                        }
                        let res = match x.body {
                            Some(crate::HttpResponseBody::Bytes(bytes)) => {
                                res.body(Box::new(bytes))
                            }
                            None => res.respond_to(&req),
                        };
                        ServiceResponse::new(req, res)
                    })
                })
                .await
        })
    }
}

struct ActixWebServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig>,
    S::Error: Into<Error>,
    S::InitError: std::fmt::Debug,
    S::Response: Into<Response<B>>,
    B: MessageBody,
{
    factory: F,
    _service: PhantomData<S>,
    _body: PhantomData<B>,
    handle: Arc<RwLock<Option<ServerHandle>>>,
    addr: String,
}

impl<F, I, S, B> WebServer for ActixWebServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig> + 'static,
    S::Error: Into<Error>,
    S::InitError: std::fmt::Debug,
    S::Response: Into<Response<B>>,
    B: MessageBody + 'static,
{
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let server = HttpServer::new(self.factory.clone());
        let server = server.bind(&self.addr).unwrap();
        let server = server.run();
        *self.handle.write().unwrap() = Some(server.handle());
        Box::pin(async move {
            if let Err(e) = server.await {
                log::error!("Error running actix server: {e:?}");
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let handle = self.handle.write().unwrap().take();
        if let Some(handle) = handle {
            return Box::pin(handle.stop(true));
        }
        Box::pin(future::ready(()))
    }
}

use crate::{HttpRequest, HttpRequestRef, WebServerBuilder};

impl WebServerBuilder {
    #[must_use]
    pub fn build_actix(self) -> Box<dyn WebServer> {
        #[cfg(feature = "cors")]
        let cors_builder = self.cors.clone();
        let factory = move || {
            #[cfg(feature = "cors")]
            let cors = {
                let cors =
                    actix_cors::Cors::default().max_age(cors_builder.max_age.map(|x| x as usize));

                let cors = match &cors_builder.allowed_origins {
                    AllOrSome::All => cors.allow_any_origin(),
                    AllOrSome::Some(origins) => {
                        let mut cors = cors;
                        for origin in origins {
                            cors = cors.allowed_origin(origin);
                        }
                        cors
                    }
                };

                let cors = match &cors_builder.allowed_methods {
                    AllOrSome::All => cors.allow_any_method(),
                    AllOrSome::Some(methods) => {
                        cors.allowed_methods(methods.iter().map(AsRef::as_ref))
                    }
                };

                let cors = match &cors_builder.allowed_headers {
                    AllOrSome::All => cors.allow_any_header(),
                    AllOrSome::Some(headers) => cors.allowed_headers(headers),
                };

                let cors = match &cors_builder.expose_headers {
                    AllOrSome::All => cors.expose_any_header(),
                    AllOrSome::Some(headers) => cors.expose_headers(headers),
                };

                let mut cors = cors;

                if cors_builder.supports_credentials {
                    cors = cors.supports_credentials();
                }

                cors
            };

            #[allow(unused_mut)]
            let mut app = {
                let mut app = actix_web::App::new();

                #[cfg(feature = "htmx")]
                let mut app = app.wrap(actix_htmx::HtmxMiddleware {});

                #[cfg(feature = "cors")]
                let mut app = app.wrap(cors);

                app
            };

            for scope in &self.scopes {
                let mut actix_scope = actix_web::web::scope(scope.path);
                for route in &scope.routes {
                    let path = route.path;
                    let route = route.clone();
                    let factory = fn_factory(move || {
                        let route = route.clone();
                        async { Ok(route) }
                    });
                    actix_scope = actix_scope.service(
                        Resource::new(path).route(actix_web::Route::new().service(factory)),
                    );
                }
                app = app.service(actix_scope);
            }

            app
        };

        Box::new(ActixWebServer {
            factory,
            _service: PhantomData,
            _body: PhantomData,
            handle: Arc::new(RwLock::new(None)),
            addr: format!("{}:{}", self.addr, self.port),
        })
    }
}
