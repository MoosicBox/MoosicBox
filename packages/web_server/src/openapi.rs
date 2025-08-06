use std::sync::{Arc, LazyLock, RwLock};

use utoipa::openapi::OpenApi;

use crate::Scope;

pub static OPENAPI: LazyLock<Arc<RwLock<Option<OpenApi>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

#[allow(dead_code)]
fn get_openapi() -> OpenApi {
    OPENAPI
        .read()
        .unwrap()
        .as_ref()
        .expect("openapi not initialized")
        .clone()
}

#[cfg(feature = "openapi-redoc")]
fn redoc_handler(
    _req: crate::HttpRequest,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
> {
    use crate::HttpResponse;

    static REDOC: LazyLock<utoipa_redoc::Redoc<OpenApi>> =
        LazyLock::new(|| utoipa_redoc::Redoc::new(get_openapi()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(REDOC.to_html())) })
}

#[cfg(feature = "openapi-scalar")]
fn scalar_handler(
    _req: crate::HttpRequest,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
> {
    use crate::HttpResponse;

    static SCALAR: LazyLock<utoipa_scalar::Scalar<OpenApi>> =
        LazyLock::new(|| utoipa_scalar::Scalar::new(get_openapi()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(SCALAR.to_html())) })
}

#[cfg(any(feature = "openapi-rapidoc", feature = "openapi-swagger-ui"))]
mod openapi_spec {
    use std::sync::LazyLock;

    use const_format::concatcp;

    use crate::{Error, HttpResponse};

    use super::get_openapi;

    pub const SPEC_URL: &str = "/swagger-ui/api-docs/openapi.json";
    const FULL_SPEC_URL: &str = concatcp!("/openapi", SPEC_URL);

    pub fn swagger_openapi_spec_handler(
        _req: crate::HttpRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
    > {
        Box::pin(async move {
            Ok(HttpResponse::ok().with_body(
                get_openapi()
                    .to_json()
                    .map_err(Error::internal_server_error)?,
            ))
        })
    }

    #[cfg(feature = "openapi-swagger-ui")]
    pub fn swagger_ui_redirect_handler(
        _req: crate::HttpRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
    > {
        Box::pin(async move {
            Ok(HttpResponse::temporary_redirect().with_location("/openapi/swagger-ui/"))
        })
    }

    #[cfg(feature = "openapi-swagger-ui")]
    pub fn swagger_ui_handler(
        req: &crate::HttpRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
    > {
        static CONFIG: LazyLock<std::sync::Arc<utoipa_swagger_ui::Config>> =
            LazyLock::new(|| std::sync::Arc::new(utoipa_swagger_ui::Config::from(FULL_SPEC_URL)));

        let path = req.path().to_string();
        Box::pin(async move {
            let path = &path[(path.find("/swagger-ui/").unwrap() + "/swagger-ui/".len())..];
            log::debug!("serving swagger-ui path='{path}'");
            match utoipa_swagger_ui::serve(path, CONFIG.clone()) {
                Ok(file) => {
                    if let Some(file) = file {
                        Ok(HttpResponse::ok().with_body(file.bytes))
                    } else {
                        Err(Error::not_found("Swagger path not found"))
                    }
                }
                Err(e) => Err(Error::internal_server_error(e)),
            }
        })
    }

    #[cfg(feature = "openapi-rapidoc")]
    pub fn rapidoc_handler(
        _req: crate::HttpRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::HttpResponse, crate::Error>> + Send>,
    > {
        static RAPIDOC: LazyLock<utoipa_rapidoc::RapiDoc> =
            LazyLock::new(|| utoipa_rapidoc::RapiDoc::new(FULL_SPEC_URL));

        Box::pin(async move { Ok(HttpResponse::ok().with_body(RAPIDOC.to_html())) })
    }
}

#[allow(clippy::let_and_return, clippy::missing_const_for_fn)]
#[must_use]
pub fn bind_services(#[allow(unused_mut)] mut scope: Scope) -> Scope {
    #[cfg(feature = "openapi-redoc")]
    {
        scope = scope.get("/redoc", redoc_handler);
    }
    #[cfg(feature = "openapi-scalar")]
    {
        scope = scope.get("/scalar", scalar_handler);
    }
    #[cfg(any(feature = "openapi-rapidoc", feature = "openapi-swagger-ui"))]
    {
        scope = scope.get(
            openapi_spec::SPEC_URL,
            openapi_spec::swagger_openapi_spec_handler,
        );
    }
    #[cfg(feature = "openapi-swagger-ui")]
    {
        scope = scope.get("/swagger-ui/{_:.*}", |req| {
            openapi_spec::swagger_ui_handler(&req)
        });
        scope = scope.get("/swagger-ui", openapi_spec::swagger_ui_redirect_handler);
    }
    #[cfg(feature = "openapi-rapidoc")]
    {
        scope = scope.get("/rapidoc", openapi_spec::rapidoc_handler);
    }
    scope
}

#[macro_export]
macro_rules! api {
    ($name:ident, $impl:expr $(,)?) => {
        $crate::paste::paste! {
            pub static [< $name:upper _API >]: std::sync::LazyLock<utoipa::openapi::OpenApi> = std::sync::LazyLock::new(|| {
                use utoipa::openapi::{*, path::*};

                $impl
            });
        }
    };
}

#[macro_export]
macro_rules! path {
    ($method:ident, $name:ident, $impl:expr $(,)?) => {
        $crate::paste::paste! {
            pub static [< $method:upper _ $name:upper _PATH >]: std::sync::LazyLock<utoipa::openapi::PathItem> =
                std::sync::LazyLock::new(|| {
                    use utoipa::openapi::{*, path::*};

                    $impl
                });
        }
    };
}
