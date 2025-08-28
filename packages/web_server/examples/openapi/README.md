# OpenAPI Integration Example

This example demonstrates comprehensive OpenAPI (Swagger) documentation integration with the MoosicBox web server. It shows how to create self-documenting APIs with interactive documentation, parameter validation, and response schemas.

## What This Example Demonstrates

- **OpenAPI 3.0 Integration**: Automatic API documentation generation
- **Interactive Documentation**: Swagger UI for API exploration and testing
- **Parameter Documentation**: Header, path, and query parameter specifications
- **Response Schemas**: Detailed response documentation with content types
- **Documentation Serving**: Hosting API docs alongside the actual API
- **Advanced Routing**: Combining documented and regular routes
- **API Organization**: Using tags and nested structures for large APIs

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic knowledge of OpenAPI/Swagger specifications
- Web browser for viewing interactive documentation

## Running the Example

### With Actix Web (Production Backend)
```bash
# From repository root
cargo run --example openapi --features "actix,openapi-all"

# From example directory
cd packages/web_server/examples/openapi
cargo run --features "actix,openapi-all"

# With NixOS
nix develop .#server --command cargo run --example openapi --features 'actix,openapi-all'
```

### With Simulator (Testing Backend)
```bash
# From repository root
cargo run --example openapi --features "simulator,openapi-all"

# From example directory
cd packages/web_server/examples/openapi
cargo run --features "simulator,openapi-all"

# With NixOS
nix develop .#server --command cargo run --example openapi --features 'simulator,openapi-all'
```

**Note**: The `openapi-all` feature enables all OpenAPI documentation formats (Swagger UI, ReDoc, RapiDoc, Scalar).

## Expected Output

The server starts with multiple endpoints:
- **API Endpoint**: `/example` - The actual API
- **Documentation**: `/openapi/swagger-ui/` - Interactive Swagger UI
- **Specification**: `/openapi/openapi.json` - Raw OpenAPI spec
- **Alternative UIs**: `/openapi/redoc/`, `/openapi/rapidoc/`, `/openapi/scalar/`

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
http://localhost:8080/openapi/redoc/      # ReDoc - Clean, responsive docs
http://localhost:8080/openapi/rapidoc/   # RapiDoc - Fast, customizable
http://localhost:8080/openapi/scalar/    # Scalar - Modern, beautiful UI
```

### API Specification

**JSON Format**
```bash
curl http://localhost:8080/openapi/openapi.json
```

**YAML Format** (if enabled)
```bash
curl http://localhost:8080/openapi/openapi.yaml
```

### Testing the Actual API

**Basic API Call**
```bash
curl http://localhost:8080/example
# Expected: JSON response with example data
```

**With Required Header**
```bash
curl -H "moosicbox-profile: test-profile" \
     http://localhost:8080/example
```

**Test Parameter Validation**
```bash
# Missing required header should return 400
curl -v http://localhost:8080/example
```

## Code Walkthrough

### OpenAPI Specification Setup

**API Definition**
```rust
#[derive(utoipa::OpenApi)]
#[openapi()]
struct ApiDoc;

pub static API: std::sync::LazyLock<utoipa::openapi::OpenApi> =
    std::sync::LazyLock::new(|| {
        OpenApi::builder()
            .tags(Some([utoipa::openapi::Tag::builder()
                .name("Example")
                .description(Some("Example API endpoints"))
                .build()]))
            .paths(/* documented paths */)
            .build()
    });
```

### Route Documentation

**Parameter Documentation**
```rust
// Header parameter
.parameter(
    Parameter::builder()
        .name("moosicbox-profile")
        .parameter_in(ParameterIn::Header)
        .description(Some("MoosicBox profile identifier"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String)))
        .build()
)

// Path parameter (for routes with {id})
.parameter(
    Parameter::builder()
        .name("magicToken")
        .parameter_in(ParameterIn::Path)
        .description(Some("The magic token to fetch credentials for"))
        .required(Required::True)
        .schema(Some(utoipa::schema!(String)))
        .build()
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
                    .description("Successful response with example data")
                    .content(
                        "application/json",
                        Content::builder()
                            .schema(Some(utoipa::schema!(Value)))
                            .build(),
                    )
                    .build(),
            ),
        )
        .response(
            "400",
            RefOr::T(
                Response::builder()
                    .description("Bad request - missing required parameters")
                    .build(),
            ),
        )
        .build(),
)
```

### Documentation Serving

**Automatic Binding**
```rust
.with_scope(
    moosicbox_web_server::openapi::bind_services(
        Scope::new("/openapi")
    )
)
```

This automatically creates:
- `/openapi/swagger-ui/` - Swagger UI interface
- `/openapi/openapi.json` - OpenAPI specification
- `/openapi/redoc/` - ReDoc interface (if enabled)
- Additional documentation formats

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

## Key Features and Benefits

### Automatic Documentation Generation
- **Always Current**: Documentation generated directly from code
- **Standard Format**: OpenAPI 3.0 specification compliance
- **Multiple Formats**: JSON, YAML, and interactive HTML interfaces
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

### Custom Response Schemas
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

### Security Schemes
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

### Nested API Organization
```rust
fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
    // Combine multiple API specifications
    // Useful for modular API design
}
```

### Error Response Documentation
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
**Solution**: Ensure `openapi-all` feature is enabled:
```bash
cargo run --features "actix,openapi-all"
```

### Documentation Not Loading
**Problem**: Swagger UI shows blank page
**Solution**: Check that the server is running and accessible at the correct port

### Missing Documentation
**Problem**: Some endpoints not appearing in docs
**Solution**: Ensure all routes are properly documented in the OpenAPI specification

### CORS Issues with Documentation
**Problem**: Documentation UI can't access API
**Solution**: CORS is configured in this example to allow all origins for development

## Comparison with Other Examples

| Example | Focus | Documentation | Interactive Testing |
|---------|-------|---------------|-------------------|
| **openapi** | API documentation | Full OpenAPI 3.0 | ✅ Swagger UI |
| simple_get | Basic routing | None | ❌ Manual only |
| nested_get | Route organization | None | ❌ Manual only |
| basic_handler | Handler patterns | None | ❌ Manual only |

## Real-World Applications

### API Versioning
```rust
// v1 API
Scope::new("/api/v1")
    .with_openapi_docs(v1_api_spec)
    .get("/users", list_users_v1)

// v2 API
Scope::new("/api/v2")
    .with_openapi_docs(v2_api_spec)
    .get("/users", list_users_v2)
```

### Microservice Documentation
```rust
// Combine multiple service specs
let combined_api = nest_api(
    main_api,
    "/auth",
    auth_service_api
).nest_api(
    "/users",
    user_service_api
);
```

### Client SDK Generation
```bash
# Generate TypeScript client
openapi-generator-cli generate \
  -i http://localhost:8080/openapi/openapi.json \
  -g typescript-axios \
  -o ./client-sdk

# Generate Python client
openapi-generator-cli generate \
  -i http://localhost:8080/openapi/openapi.json \
  -g python \
  -o ./python-client
```

## Architecture Overview

```
┌─────────────────────────────────────┐
│           HTTP Request              │
├─────────────────────────────────────┤
│            CORS Layer               │
├─────────────────────────────────────┤
│         OpenAPI Routes              │
│  /openapi/swagger-ui/    (Swagger)  │
│  /openapi/redoc/         (ReDoc)    │
│  /openapi/rapidoc/       (RapiDoc)  │
│  /openapi/scalar/        (Scalar)   │
│  /openapi/openapi.json   (Spec)     │
├─────────────────────────────────────┤
│          API Routes                 │
│  /example               (Documented)│
│  /health                (Regular)   │
└─────────────────────────────────────┘
```

## Related Examples

- **basic_handler**: Foundation for handler patterns
- **simple_get**: Basic routing without documentation
- **nested_get**: Route organization patterns
- **query_extractor**: Parameter extraction techniques
- **json_extractor**: Request body handling
- **combined_extractors**: Complex parameter handling

## Next Steps

### Enhance This Example
1. **Add Authentication**: Document security schemes and protected endpoints
2. **Custom Schemas**: Create typed request/response models
3. **Error Handling**: Document all possible error responses
4. **Validation**: Add request validation with documented constraints

### Production Deployment
1. **Environment Configuration**: Different docs for dev/staging/prod
2. **Access Control**: Restrict documentation access in production
3. **Performance**: Optimize documentation serving for production loads
4. **Monitoring**: Track API usage through documented endpoints

This example provides the foundation for creating professional, self-documenting APIs with the MoosicBox web server framework.
