use moosicbox_web_server::{HttpResponse, Scope};
use utoipa::openapi::{OpenApi, Response};

mod openapi;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None, None)?;

    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server = moosicbox_web_server::WebServerBuilder::new()
        .with_cors(cors)
        .with_scope(openapi::bind_services(Scope::new("/openapi")))
        // The order matters here. Make sure to add the root scope last
        .with_scope(Scope::new("").with_route(GET_EXAMPLE))
        .build_actix();

    server.start().await;

    Ok(())
}

pub static API: std::sync::LazyLock<utoipa::openapi::OpenApi> = std::sync::LazyLock::new(|| {
    OpenApi::builder()
        .tags(Some([utoipa::openapi::Tag::builder()
            .name("Example")
            .build()]))
        .paths(
            utoipa::openapi::Paths::builder()
                .path("/example", GET_EXAMPLE_OPENAPI.clone())
                .build(),
        )
        .components(Some(utoipa::openapi::Components::builder().build()))
        .build()
});

pub static GET_EXAMPLE_OPENAPI: std::sync::LazyLock<utoipa::openapi::PathItem> =
    std::sync::LazyLock::new(|| {
        use utoipa::openapi::{
            Content, HttpMethod, ObjectBuilder, RefOr, Required, Responses, Type,
            path::{Operation, Parameter, ParameterIn},
        };

        utoipa::openapi::PathItem::builder()
            .operation(
                HttpMethod::Get,
                Operation::builder()
                    .description(Some("description"))
                    .tags(Some(["Tag1", "Tag2"]))
                    .parameter(
                        Parameter::builder()
                            .name("moosicbox-profile")
                            .parameter_in(ParameterIn::Header)
                            .description(Some("MoosicBox profile"))
                            .required(Required::True)
                            .schema(Some(ObjectBuilder::new().schema_type(Type::String))),
                    )
                    .parameter(
                        Parameter::builder()
                            .name("magicToken")
                            .parameter_in(ParameterIn::Path)
                            .description(Some("The magic token to fetch the credentials for"))
                            .required(Required::True)
                            .schema(Some(ObjectBuilder::new().schema_type(Type::String))),
                    )
                    .responses(
                        Responses::builder()
                            .response(
                                "200",
                                RefOr::T(
                                    Response::builder()
                                        .description("The credentials for the magic token")
                                        .content(
                                            "application/json",
                                            Content::builder()
                                                .schema(Some(
                                                    ObjectBuilder::new().schema_type(Type::Object),
                                                ))
                                                .build(),
                                        )
                                        .build(),
                                ),
                            )
                            .build(),
                    ),
            )
            .build()
    });

moosicbox_web_server::route!(GET, example, "/example", |req| {
    Box::pin(async move {
        Ok(HttpResponse::ok().with_body(format!(
            "hello, world! path={} query={}",
            req.path(),
            req.query_string()
        )))
    })
});
