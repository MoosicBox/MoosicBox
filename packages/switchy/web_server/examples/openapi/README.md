# OpenAPI Integration Example

This example demonstrates comprehensive OpenAPI (Swagger) documentation integration with the MoosicBox web server. It shows how to create self-documenting APIs with interactive documentation, parameter validation, and response schemas.

## What This Example Demonstrates

- **OpenAPI 3.0 Integration**: Automatic API documentation generation
- **Interactive Documentation**: Swagger UI and alternative documentation UIs (ReDoc, RapiDoc, Scalar)
- **Parameter Documentation**: Header, path, and query parameter specifications
- **Response Schemas**: Detailed response documentation with content types
- **Documentation Serving**: Hosting API docs alongside the actual API using `bind_services`
- **API Organization**: Using tags and the `nest_api` pattern for complex API structures
- **Documentation-First Approach**: Shows how to write comprehensive API documentation using the `path!` macro

**Important**: The OpenAPI documentation in this example intentionally does not match the simple handler implementation. This demonstrates how to write documentation specifications independently, which is useful for:

- Planning APIs before implementation
- Documenting expected behavior for future development
- Learning the OpenAPI documentation patterns without implementation complexity

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic knowledge of OpenAPI/Swagger specifications
- Web browser for viewing interactive documentation

## Running the Example

### With Actix Web (Production Backend)

```bash
# From repository root
cargo run --package web_server_openapi
# or using the short form:
cargo run -p web_server_openapi

# From example directory
cd packages/web_server/examples/openapi
cargo run

# With NixOS
nix develop .#server --command cargo run -p web_server_openapi
```

**Note**: This example uses the Actix Web backend with all OpenAPI documentation formats (Swagger UI, ReDoc, RapiDoc, Scalar) enabled by default. To use a different backend or customize features, you'll need to modify the `Cargo.toml` dependencies.

## Expected Output

The server starts with multiple endpoints:

- **API Endpoint**: `/example` - The actual API
- **Documentation**: `/openapi/swagger-ui/` - Interactive Swagger UI
- **Specification**: `/openapi/swagger-ui/api-docs/openapi.json` - Raw OpenAPI spec
- **Alternative UIs**: `/openapi/redoc`, `/openapi/rapidoc`, `/openapi/scalar`

## Testing the API and Documentation

### Interactive Documentation

**Swagger UI (Primary Interface)**

```
http://localhost:8080/openapi/swagger-ui/
```

- Interactive API exploration
- Direct request testing
- Parameter input forms
- Response visualization

**Alternative Documentation UIs**

```
http://localhost:8080/openapi/redoc      # ReDoc - Clean, responsive docs
http://localhost:8080/openapi/rapidoc   # RapiDoc - Fast, customizable
http://localhost:8080/openapi/scalar    # Scalar - Modern, beautiful UI
```

### API Specification

```bash
curl http://localhost:8080/openapi/swagger-ui/api-docs/openapi.json
```

### Testing the Actual API

**Basic API Call**

```bash
curl http://localhost:8080/example
# Expected: Plain text response showing path and query string
# Example: "hello, world! path=/example query="
```

**Note**: The actual `/example` endpoint implementation is intentionally simple and does not perform parameter validation. The OpenAPI documentation showcases how to document APIs with headers, path parameters, and validation requirements, but the handler itself doesn't enforce these constraints. This demonstrates the documentation capabilities independently from the implementation.

## Code Walkthrough

### OpenAPI Specification Setup

**API Definition**

```rust
pub static API: std::sync::LazyLock<utoipa::openapi::OpenApi> =
    std::sync::LazyLock::new(|| {
        OpenApi::builder()
            .tags(Some([utoipa::openapi::Tag::builder()
                .name("Example")
                .build()]))
            .paths(
                utoipa::openapi::Paths::builder()
                    .path("/example", GET_EXAMPLE_PATH.clone())
                    .build(),
            )
            .components(Some(utoipa::openapi::Components::builder().build()))
            .build()
    });

#[derive(utoipa::OpenApi)]
#[openapi()]
struct ApiDoc;
```

### Route Documentation

The example uses the `path!` macro to define OpenAPI documentation for the `/example` endpoint:

**Parameter Documentation**

```rust
.parameter(
    Parameter::builder()
        .name("moosicbox-profile")
        .parameter_in(ParameterIn::Header)
        .description(Some("MoosicBox profile"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String))),
)
.parameter(
    Parameter::builder()
        .name("magicToken")
        .parameter_in(ParameterIn::Path)
        .description(Some("The magic token to fetch the credentials for"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String))),
)
```

**Response Documentation**

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

**Important**: This documentation is for demonstration purposes. The actual handler is intentionally simple and doesn't validate these parameters or return JSON. This showcases how OpenAPI documentation can be written independently of implementation.

### Documentation Serving

**Automatic Binding**

```rust
.with_scope(moosicbox_web_server::openapi::bind_services(
    Scope::new("/openapi")
))
```

This automatically creates:

- `/openapi/swagger-ui/` - Swagger UI interface
- `/openapi/swagger-ui/api-docs/openapi.json` - OpenAPI specification
- `/openapi/redoc` - ReDoc interface
- `/openapi/rapidoc` - RapiDoc interface
- `/openapi/scalar` - Scalar interface

## API Documentation Structure

### Tags and Organization

```rust
.tags(Some([utoipa::openapi::Tag::builder()
    .name("Example")
    .build()]))
```

Tags are used to organize endpoints into logical groups in the documentation UI.

### Nested API Structure

The example includes a `nest_api` function that demonstrates how to combine multiple API specifications:

```rust
fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
    nested.paths.paths.iter_mut().for_each(|(path, item)| {
        item.options.iter_mut().for_each(|operation| {
            operation.operation_id = Some(path.to_owned());
        });
    });

    api.nest(path, nested)
}
```

This pattern supports hierarchical API organization for complex applications with multiple service specifications.

## Key Features and Benefits

### Automatic Documentation Generation

- **Always Current**: Documentation generated directly from code
- **Standard Format**: OpenAPI 3.0 specification compliance
- **Multiple Formats**: JSON specification and interactive HTML interfaces
- **Zero Maintenance**: No separate documentation to maintain

### Interactive Testing

- **Built-in Testing**: Test APIs directly from the documentation
- **Parameter Validation**: Real-time validation of request parameters
- **Response Visualization**: Formatted response display
- **Multiple UIs**: Choose from Swagger UI, ReDoc, RapiDoc, or Scalar

### Developer Experience

- **Self-Documenting**: Code serves as the source of truth
- **Type Safety**: Schema validation and type checking
- **Client Generation**: Generate client libraries in multiple languages
- **API Exploration**: Easy discovery of available endpoints

### Production Benefits

- **API Governance**: Standardized API specifications
- **Version Control**: Documentation versioned with code
- **Team Collaboration**: Shared understanding of API contracts
- **Integration Testing**: Automated testing against documented schemas

## Advanced OpenAPI Patterns

### Custom Response Schemas (Example Pattern)

```rust
#[derive(utoipa::ToSchema, serde::Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

// Use in OpenAPI documentation
.schema(Some(utoipa::schema!(ApiResponse)))
```

### Security Schemes (Example Pattern)

```rust
.components(Some(
    utoipa::openapi::Components::builder()
        .security_scheme("bearer_auth", SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format(Some("JWT"))
                .description(Some("JWT Bearer token authentication"))
                .build()
        ))
        .build()
))

// Apply to specific operations
.security(Some([SecurityRequirement::new("bearer_auth", ["read", "write"])]))
```

### Nested API Organization ✅ Implemented

The `nest_api` function is implemented in this example:

```rust
fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
    nested.paths.paths.iter_mut().for_each(|(path, item)| {
        item.options.iter_mut().for_each(|operation| {
            operation.operation_id = Some(path.to_owned());
        });
    });

    api.nest(path, nested)
}
```

This pattern is useful for combining multiple API specifications in modular designs.

### Error Response Documentation (Example Pattern)

```rust
.response(
    "404",
    RefOr::T(
        Response::builder()
            .description("Resource not found")
            .content(
                "application/json",
                Content::builder()
                    .schema(Some(utoipa::schema!(ErrorResponse)))
                    .build(),
            )
            .build(),
    ),
)
```

## Troubleshooting

### Feature Flag Issues

**Problem**: OpenAPI features not available
**Solution**: This example has `openapi-all` enabled by default in its `Cargo.toml`. If you've modified the dependencies, ensure `moosicbox_web_server` is included with the `openapi-all` feature:

```toml
moosicbox_web_server = { workspace = true, features = ["actix", "cors", "openapi-all"] }
```

### Documentation Not Loading

**Problem**: Swagger UI shows blank page
**Solution**:

- Verify the server is running and accessible at `http://localhost:8080`
- Check that the OpenAPI spec is available at `http://localhost:8080/openapi/swagger-ui/api-docs/openapi.json`
- Look for any error messages in the server console

### Missing Documentation

**Problem**: Some endpoints not appearing in docs
**Solution**: Ensure all routes are properly added to the OpenAPI specification in the `API` static variable and registered via the `path!` macro

### CORS Issues with Documentation

**Problem**: Documentation UI can't access API
**Solution**: CORS is configured in this example to allow all origins for development

## Comparison with Other Examples

| Example       | Focus              | Documentation    | Interactive Testing |
| ------------- | ------------------ | ---------------- | ------------------- |
| **openapi**   | API documentation  | Full OpenAPI 3.0 | ✅ Swagger UI       |
| simple_get    | Basic routing      | None             | ❌ Manual only      |
| nested_get    | Route organization | None             | ❌ Manual only      |
| basic_handler | Handler patterns   | None             | ❌ Manual only      |

## Real-World Applications

### API Versioning (Example Pattern)

```rust
// v1 API
Scope::new("/api/v1")
    .get("/users", list_users_v1)

// v2 API
Scope::new("/api/v2")
    .get("/users", list_users_v2)

// Combine their OpenAPI specs
let v1_api = /* build v1 OpenAPI spec */;
let v2_api = /* build v2 OpenAPI spec */;
```

### Microservice Documentation (Example Pattern)

```rust
// Combine multiple service specs using the nest_api pattern
// demonstrated in this example
let combined_api = nest_api(
    main_api,
    "/auth",
    auth_service_api
);
```

### Client SDK Generation

Once you have your OpenAPI specification running, you can generate client SDKs:

```bash
# Generate TypeScript client
openapi-generator-cli generate \
  -i http://localhost:8080/openapi/swagger-ui/api-docs/openapi.json \
  -g typescript-axios \
  -o ./client-sdk

# Generate Python client
openapi-generator-cli generate \
  -i http://localhost:8080/openapi/swagger-ui/api-docs/openapi.json \
  -g python \
  -o ./python-client
```

**Note**: The patterns above are examples of how you could extend this approach. This example demonstrates the core OpenAPI documentation mechanics.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    HTTP Request                         │
├─────────────────────────────────────────────────────────┤
│                     CORS Layer                          │
├─────────────────────────────────────────────────────────┤
│                  OpenAPI Routes                         │
│  /openapi/swagger-ui/                        (Swagger)  │
│  /openapi/redoc                              (ReDoc)    │
│  /openapi/rapidoc                            (RapiDoc)  │
│  /openapi/scalar                             (Scalar)   │
│  /openapi/swagger-ui/api-docs/openapi.json   (Spec)     │
├─────────────────────────────────────────────────────────┤
│                   API Routes                            │
│  /example                                   (Documented)│
└─────────────────────────────────────────────────────────┘
```

## Related Examples

- **basic_handler**: Foundation for handler patterns
- **simple_get**: Basic routing without documentation
- **nested_get**: Route organization patterns
- **query_extractor_standalone**: Parameter extraction techniques
- **json_extractor_standalone**: Request body handling
- **combined_extractors_standalone**: Complex parameter handling

## Next Steps

### Enhance This Example

To make this example production-ready, you could:

1. **Match Implementation to Docs**: Update the handler to actually validate the documented parameters
2. **Add Authentication**: Implement and document security schemes for protected endpoints
3. **Custom Schemas**: Create typed request/response models with `#[derive(utoipa::ToSchema)]`
4. **Error Handling**: Implement proper error responses that match the documented schemas
5. **Validation**: Add request validation that enforces the documented constraints

### Learning Path

After understanding this example:

1. Review the **basic_handler** example for handler implementation patterns
2. Explore the **json_extractor_standalone** example to handle JSON request bodies
3. Study the **combined_extractors_standalone** example for parameter validation
4. Apply OpenAPI documentation to those patterns

### Production Deployment Considerations

1. **Environment Configuration**: Consider different documentation settings for dev/staging/prod
2. **Access Control**: Restrict documentation access in production environments
3. **Performance**: The documentation is generated once at startup and served efficiently
4. **Monitoring**: Use the OpenAPI spec to generate API usage metrics and monitoring

This example provides the foundation for creating professional, self-documenting APIs with the MoosicBox web server framework.
