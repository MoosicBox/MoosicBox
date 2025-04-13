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
crate::route!(GET, redoc, "/redoc", |_req| {
    use crate::HttpResponse;

    static REDOC: LazyLock<utoipa_redoc::Redoc<OpenApi>> =
        LazyLock::new(|| utoipa_redoc::Redoc::new(get_openapi()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(REDOC.to_html())) })
});

#[cfg(feature = "openapi-scalar")]
crate::route!(GET, scalar, "/scalar", |_req| {
    use crate::HttpResponse;

    static SCALAR: LazyLock<utoipa_scalar::Scalar<OpenApi>> =
        LazyLock::new(|| utoipa_scalar::Scalar::new(get_openapi()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(SCALAR.to_html())) })
});

#[cfg(any(feature = "openapi-rapidoc", feature = "openapi-swagger-ui"))]
mod openapi_spec {
    use std::sync::LazyLock;

    use const_format::concatcp;

    use crate::{Error, HttpResponse, route};

    use super::get_openapi;

    const SPEC_URL: &str = "/swagger-ui/api-docs/openapi.json";
    const FULL_SPEC_URL: &str = concatcp!("/openapi", SPEC_URL);

    route!(GET, swagger_openapi_spec, SPEC_URL, |_req| {
        Box::pin(async move {
            Ok(HttpResponse::ok().with_body(
                get_openapi()
                    .to_json()
                    .map_err(Error::internal_server_error)?,
            ))
        })
    });

    #[cfg(feature = "openapi-swagger-ui")]
    route!(GET, swagger_ui_redirect, "/swagger-ui", |_req| {
        Box::pin(async move {
            Ok(HttpResponse::temporary_redirect().with_location("/openapi/swagger-ui/"))
        })
    });

    #[cfg(feature = "openapi-swagger-ui")]
    route!(GET, swagger_ui, "/swagger-ui/{_:.*}", |req| {
        static CONFIG: LazyLock<std::sync::Arc<utoipa_swagger_ui::Config>> =
            LazyLock::new(|| std::sync::Arc::new(utoipa_swagger_ui::Config::from(FULL_SPEC_URL)));

        Box::pin(async move {
            let path = req.path();
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
                Err(e) => Err(Error::InternalServerError(e)),
            }
        })
    });

    #[cfg(feature = "openapi-rapidoc")]
    route!(GET, rapidoc, "/rapidoc", |_req| {
        static RAPIDOC: LazyLock<utoipa_rapidoc::RapiDoc> =
            LazyLock::new(|| utoipa_rapidoc::RapiDoc::new(FULL_SPEC_URL));

        Box::pin(async move { Ok(HttpResponse::ok().with_body(RAPIDOC.to_html())) })
    });
}

#[allow(clippy::let_and_return, clippy::missing_const_for_fn)]
#[must_use]
pub fn bind_services(scope: Scope) -> Scope {
    #[cfg(feature = "openapi-redoc")]
    let scope = scope.with_route(GET_REDOC);
    #[cfg(feature = "openapi-scalar")]
    let scope = scope.with_route(GET_SCALAR);
    #[cfg(any(feature = "openapi-rapidoc", feature = "openapi-swagger-ui"))]
    let scope = scope.with_route(openapi_spec::GET_SWAGGER_OPENAPI_SPEC);
    #[cfg(feature = "openapi-swagger-ui")]
    let scope = scope.with_route(openapi_spec::GET_SWAGGER_UI);
    #[cfg(feature = "openapi-swagger-ui")]
    let scope = scope.with_route(openapi_spec::GET_SWAGGER_UI_REDIRECT);
    #[cfg(feature = "openapi-rapidoc")]
    let scope = scope.with_route(openapi_spec::GET_RAPIDOC);
    scope
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
