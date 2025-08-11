# Web Server OpenAPI Example

A demonstration of OpenAPI documentation generation for MoosicBox web server.

## Overview

This example shows how to integrate OpenAPI (Swagger) documentation with the MoosicBox web server framework. It demonstrates automatic API documentation generation, interactive documentation serving, and proper API specification structure.

## What it demonstrates

- **OpenAPI integration** - Automatic API documentation generation
- **Interactive documentation** - Swagger UI for API exploration
- **API specification** - Proper OpenAPI 3.0 document structure
- **Route documentation** - Documenting endpoints with parameters and responses
- **Documentation serving** - Hosting API docs alongside the API
- **Advanced routing** - Combining documented and regular routes

## Code walkthrough

The example:

1. **Sets up logging** and CORS configuration
2. **Initializes OpenAPI specification** with route documentation
3. **Creates documented routes** with parameters and response schemas
4. **Serves OpenAPI documentation** at `/openapi` endpoints
5. **Combines documented and regular routes** in the same server

## Key concepts

### OpenAPI Specification

```rust
#[derive(utoipa::OpenApi)]
#[openapi()]
struct ApiDoc;

pub static API: std::sync::LazyLock<utoipa::openapi::OpenApi> =
    std::sync::LazyLock::new(|| {
        OpenApi::builder()
            .tags(Some([utoipa::openapi::Tag::builder()
                .name("Example")
                .build()]))
            .paths(/* ... */)
            .build()
    });
```

Defining the API specification structure with tags and paths.

### Route Documentation

```rust
path!(
    GET,
    example,
    utoipa::openapi::PathItem::builder()
        .operation(
            HttpMethod::Get,
            Operation::builder()
                .description(Some("description"))
                .tags(Some(["Tag1", "Tag2"]))
                .parameter(/* header parameter */)
                .parameter(/* path parameter */)
                .responses(/* response definitions */)
        )
        .build()
);
```

Comprehensive route documentation with parameters and responses.

### Documentation Serving

```rust
.with_scope(moosicbox_web_server::openapi::bind_services(Scope::new("/openapi")))
```

Automatically binding OpenAPI documentation endpoints.

## Prerequisites

⚠️ **Important**: This example requires the `serde` feature to be enabled because the FromRequest implementations use `serde_json` for parsing.

## Running the example

```bash
# From repository root
cargo run -p web_server_openapi --features "moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run -p web_server_openapi --features 'moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/openapi
cargo run --features "moosicbox_web_server/serde"
```

## Build only (for testing compilation)

```bash
# Build the example
cargo build -p web_server_openapi --features "moosicbox_web_server/serde"
```

## Troubleshooting

### Missing serde feature error
If you see `use of unresolved module or unlinked crate 'serde_json'`, make sure to include the serde feature:
```bash
--features "moosicbox_web_server/serde"
```

### Package name error
The package name is `web_server_openapi`, not `openapi`.

The server will start with both the API and documentation endpoints.

## Accessing the documentation

### Interactive Swagger UI

Open your browser and navigate to:
```
http://localhost:8080/openapi/swagger-ui/
```

This provides an interactive interface to:
- **Explore API endpoints**
- **Test requests directly**
- **View request/response schemas**
- **Download the OpenAPI specification**

### Raw OpenAPI Specification

```bash
curl http://localhost:8080/openapi/openapi.json
```

Returns the complete OpenAPI 3.0 specification in JSON format.

### API Endpoint

```bash
curl http://localhost:8080/example
```

The actual API endpoint that's documented.

## OpenAPI Features

### Parameter Documentation

The example documents two parameter types:

#### Header Parameter

```rust
.parameter(
    Parameter::builder()
        .name("moosicbox-profile")
        .parameter_in(ParameterIn::Header)
        .description(Some("MoosicBox profile"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String))),
)
```

#### Path Parameter

```rust
.parameter(
    Parameter::builder()
        .name("magicToken")
        .parameter_in(ParameterIn::Path)
        .description(Some("The magic token to fetch the credentials for"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String))),
)
```

### Response Documentation

```rust
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
                            .schema(Some(utoipa::schema!(Value)))
                            .build(),
                    )
                    .build(),
            ),
        )
        .build(),
)
```

Complete response documentation with content types and schemas.

## API Documentation Structure

### Tags and Organization

```rust
.tags(Some([utoipa::openapi::Tag::builder()
    .name("Example")
    .build()]))
```

Organizing endpoints into logical groups.

### Nested API Structure

```rust
fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
    // Nesting logic for complex API organization
}
```

Supporting hierarchical API organization.

## Use cases

This pattern is useful for:

- **API documentation** - Automatic, up-to-date documentation
- **Developer experience** - Interactive API exploration
- **API testing** - Built-in testing interface
- **Client generation** - Code generation from OpenAPI specs
- **API governance** - Standardized API specifications

## Benefits

### Automatic Documentation

- **Always up-to-date** - Documentation generated from code
- **Consistent format** - Standard OpenAPI 3.0 specification
- **Interactive testing** - Swagger UI for immediate testing
- **Multiple formats** - JSON, YAML, and interactive HTML

### Developer Experience

- **Self-documenting APIs** - Code serves as documentation
- **Type safety** - Schema validation and type checking
- **Testing integration** - Built-in API testing capabilities
- **Client generation** - Generate clients in multiple languages

## Advanced Features

### Custom Schemas

```rust
#[derive(utoipa::ToSchema, serde::Serialize)]
struct CustomResponse {
    message: String,
    data: Vec<String>,
}
```

Define custom response schemas for complex data types.

### Security Definitions

```rust
.components(Some(
    utoipa::openapi::Components::builder()
        .security_scheme("bearer", SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format(Some("JWT"))
                .build()
        ))
        .build()
))
```

Document authentication and authorization requirements.

## Dependencies

- `moosicbox_web_server` - Web server with OpenAPI support
- `utoipa` - OpenAPI specification generation
- `moosicbox_logging` - Logging setup
- `tokio` - Async runtime

## Comparison with other examples

| Example | Focus | Documentation |
|---------|-------|---------------|
| [Simple GET](../simple_get/README.md) | Basic routing | None |
| [Nested GET](../nested_get/README.md) | Route organization | None |
| **OpenAPI** | API documentation | Full OpenAPI 3.0 |

## Server Architecture

```
┌─────────────────┐
│  HTTP Request   │
├─────────────────┤
│   CORS Layer    │
├─────────────────┤
│ OpenAPI Routes  │ ← /openapi/swagger-ui/
│                 │ ← /openapi/openapi.json
├─────────────────┤
│  API Routes     │ ← /example
└─────────────────┘
```

## Related Packages

- [`moosicbox_web_server`](../../README.md) - Main web server package
- [`moosicbox_server`](../../../server/README.md) - Full server with OpenAPI
- [`moosicbox_auth`](../../../auth/README.md) - Authentication documentation
- [`moosicbox_middleware`](../../../middleware/README.md) - Middleware documentation

## Next Steps

1. **Add more endpoints** with comprehensive documentation
2. **Implement authentication** and document security schemes
3. **Create custom schemas** for complex data types
4. **Generate client libraries** from the OpenAPI specification
