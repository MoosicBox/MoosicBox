use std::sync::{Arc, LazyLock};

use const_format::concatcp;
use moosicbox_web_server::{HttpResponse, Scope, WebServerError, route};
use utoipa::{OpenApi as _, openapi::OpenApi};
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::Redoc;
use utoipa_scalar::Scalar;
use utoipa_swagger_ui::Config;

use crate::API;

#[derive(utoipa::OpenApi)]
#[openapi()]
struct ApiDoc;

pub fn init() -> OpenApi {
    #[allow(unused)]
    fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
        nested.paths.paths.iter_mut().for_each(|(path, item)| {
            item.options.iter_mut().for_each(|operation| {
                operation.operation_id = Some(path.to_owned());
            });
        });

        api.nest(path, nested)
    }

    nest_api(ApiDoc::openapi(), "", API.clone())
}

static OPENAPI: LazyLock<OpenApi> = LazyLock::new(init);

route!(GET, redoc, "/redoc", |_req| {
    static REDOC: LazyLock<Redoc<OpenApi>> =
        LazyLock::new(|| utoipa_redoc::Redoc::new(OPENAPI.clone()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(REDOC.to_html())) })
});

const SPEC_URL: &str = "/swagger-ui/api-docs/openapi.json";
const FULL_SPEC_URL: &str = concatcp!("/openapi", SPEC_URL);

route!(GET, swagger_openapi_spec, SPEC_URL, |_req| {
    Box::pin(async move {
        Ok(HttpResponse::ok().with_body(
            OPENAPI
                .to_json()
                .map_err(|_e| WebServerError::InternalServerError)?,
        ))
    })
});

route!(GET, swagger_ui_redirect, "/swagger-ui", |_req| {
    Box::pin(
        async move { Ok(HttpResponse::temporary_redirect().with_location("/openapi/swagger-ui/")) },
    )
});

route!(GET, swagger_ui, "/swagger-ui/{_:.*}", |req| {
    static CONFIG: LazyLock<Arc<Config>> = LazyLock::new(|| Arc::new(Config::from(FULL_SPEC_URL)));

    Box::pin(async move {
        let path = req.path();
        let path = &path[(path.find("/swagger-ui/").unwrap() + "/swagger-ui/".len())..];
        log::debug!("serving swagger-ui path='{path}'");
        match utoipa_swagger_ui::serve(path, CONFIG.clone()) {
            Ok(file) => {
                if let Some(file) = file {
                    Ok(HttpResponse::ok().with_body(file.bytes))
                } else {
                    Err(WebServerError::NotFound)
                }
            }
            Err(_error) => Err(WebServerError::InternalServerError),
        }
    })
});

route!(GET, rapidoc, "/rapidoc", |_req| {
    static RAPIDOC: LazyLock<RapiDoc> =
        LazyLock::new(|| utoipa_rapidoc::RapiDoc::new(FULL_SPEC_URL));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(RAPIDOC.to_html())) })
});

route!(GET, scalar, "/scalar", |_req| {
    static SCALAR: LazyLock<Scalar<OpenApi>> =
        LazyLock::new(|| utoipa_scalar::Scalar::new(OPENAPI.clone()));

    Box::pin(async move { Ok(HttpResponse::ok().with_body(SCALAR.to_html())) })
});

pub fn bind_services(scope: Scope) -> Scope {
    scope
        .with_route(GET_SWAGGER_OPENAPI_SPEC)
        .with_route(GET_REDOC)
        .with_route(GET_SWAGGER_UI)
        .with_route(GET_SWAGGER_UI_REDIRECT)
        .with_route(GET_RAPIDOC)
        .with_route(GET_SCALAR)
}
