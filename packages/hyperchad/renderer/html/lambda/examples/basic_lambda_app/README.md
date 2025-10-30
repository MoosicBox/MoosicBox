# Basic AWS Lambda Application Example

A complete serverless web application demonstrating HyperChad Lambda renderer integration with AWS Lambda, API Gateway, and Application Load Balancers.

## What This Example Demonstrates

- Implementing the `LambdaResponseProcessor` trait for custom request handling
- Routing HTTP requests based on method and path
- Generating HTML page responses with inline CSS styling
- Serving JSON API endpoints for health checks and service information
- Adding custom HTTP headers (cache control, security headers)
- Handling 404 errors with custom error pages
- Using automatic gzip compression for all responses
- Integrating with AWS Lambda runtime and HTTP events

## Prerequisites

Before running this example, you should have:

- Basic understanding of AWS Lambda and serverless architectures
- Rust toolchain installed (1.70+)
- AWS account and AWS CLI configured (for deployment)
- Familiarity with HTTP request/response concepts
- Understanding of async Rust programming

For local testing, you'll need:

- Docker (for AWS Lambda runtime emulation)
- [cargo-lambda](https://github.com/cargo-lambda/cargo-lambda) CLI tool

## Running the Example

### Local Development with cargo-lambda

First, install cargo-lambda:

```bash
pip3 install cargo-lambda
# or
brew tap cargo-lambda/cargo-lambda
brew install cargo-lambda
```

Then run the example locally:

```bash
# From the repository root
cd packages/hyperchad/renderer/html/lambda/examples/basic_lambda_app

# Start the local Lambda runtime
cargo lambda watch

# In another terminal, invoke the function
curl http://localhost:9000/lambda-url/basic_lambda_app_example/
curl http://localhost:9000/lambda-url/basic_lambda_app_example/about
curl http://localhost:9000/lambda-url/basic_lambda_app_example/api/health
```

### Building for AWS Lambda Deployment

Build the Lambda deployment package:

```bash
# Build for Lambda environment
cargo lambda build --release --manifest-path packages/hyperchad/renderer/html/lambda/examples/basic_lambda_app/Cargo.toml

# The binary will be in target/lambda/basic_lambda_app_example/
```

### Deploying to AWS Lambda

Deploy using cargo-lambda:

```bash
cargo lambda deploy basic_lambda_app_example \
  --manifest-path packages/hyperchad/renderer/html/lambda/examples/basic_lambda_app/Cargo.toml
```

Or deploy using AWS SAM/CDK with the built binary from `target/lambda/`.

## Expected Output

### Home Page (GET /)

When you visit the root URL, you'll see an HTML page with:

- Welcome message and introduction
- Navigation links to other routes
- Feature list with checkmarks
- Styled content with CSS

**Example curl output:**

```bash
curl http://localhost:9000/lambda-url/basic_lambda_app_example/
```

Returns HTML starting with:

```html
<!DOCTYPE html>
<html lang="en">
    <head>
        <title>HyperChad Lambda Example</title>
        ...
    </head>
    <body>
        <h1>Welcome to HyperChad on AWS Lambda!</h1>
        ...
    </body>
</html>
```

### About Page (GET /about)

```bash
curl http://localhost:9000/lambda-url/basic_lambda_app_example/about
```

Returns an HTML page explaining HyperChad Lambda Renderer features and technology stack.

### Health Check API (GET /api/health)

```bash
curl http://localhost:9000/lambda-url/basic_lambda_app_example/api/health
```

Returns JSON:

```json
{
    "status": "healthy",
    "service": "hyperchad-lambda",
    "timestamp": "2025-10-30T12:34:56.789Z"
}
```

### Service Info API (GET /api/info)

```bash
curl http://localhost:9000/lambda-url/basic_lambda_app_example/api/info
```

Returns JSON:

```json
{
    "version": "1.0.0",
    "renderer": "hyperchad_renderer_html_lambda",
    "features": ["html", "json", "gzip-compression"]
}
```

### 404 Not Found

```bash
curl http://localhost:9000/lambda-url/basic_lambda_app_example/unknown
```

Returns a styled 404 error page with a link back to home.

## Code Walkthrough

### Main Application Setup

```rust
#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let processor = SimpleProcessor;
    let app = LambdaApp::new(processor);
    let handle = hyperchad_renderer::Handle::current();
    let mut runner = app.to_runner(handle)?;
    runner.run()?;
    Ok(())
}
```

The main function initializes logging, creates a processor, builds the Lambda app, and runs the event loop.

### Request Processor Implementation

```rust
#[derive(Clone)]
struct SimpleProcessor;

#[async_trait]
impl LambdaResponseProcessor<(String, String)> for SimpleProcessor {
    fn prepare_request(&self, req: Request, _body: Option<Arc<Bytes>>)
        -> Result<(String, String), Error>
    {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        Ok((method, path))
    }
    // ... other methods
}
```

The processor extracts HTTP method and path from incoming requests. This data type `(String, String)` is passed through all handler methods.

### Routing Logic

```rust
async fn to_response(&self, (method, path): (String, String))
    -> Result<Option<(Content, Option<Vec<(String, String)>>)>, Error>
{
    match (method.as_str(), path.as_str()) {
        ("GET", "/") => Ok(Some((Content::Html(render_home_page()), None))),
        ("GET", "/about") => Ok(Some((Content::Html(render_about_page()), None))),
        ("GET", "/api/health") => Ok(Some((Content::Json(json!({ ... })), None))),
        _ => Ok(Some((Content::Html(render_404_page(&path)), None))),
    }
}
```

Routing matches on method and path tuples, returning appropriate `Content::Html` or `Content::Json` responses.

### Custom Headers

```rust
fn headers(&self, _content: &hyperchad_renderer::Content)
    -> Option<Vec<(String, String)>>
{
    Some(vec![
        ("X-Powered-By".to_string(), "HyperChad".to_string()),
        ("Cache-Control".to_string(), "public, max-age=300".to_string()),
    ])
}
```

Headers are added to every response, enabling caching and custom metadata.

### HTML Rendering

HTML is generated using string literals with inline CSS. In production, you'd typically use the HyperChad template DSL:

```rust
fn render_home_page() -> String {
    r#"<!DOCTYPE html>
    <html lang="en">
    <head>
        <title>HyperChad Lambda Example</title>
        <style>
            body { font-family: sans-serif; }
        </style>
    </head>
    <body>
        <h1>Welcome!</h1>
    </body>
    </html>"#.to_string()
}
```

## Key Concepts

### LambdaResponseProcessor Trait

The `LambdaResponseProcessor` trait provides four key methods:

1. **`prepare_request`** - Extracts data from incoming Lambda HTTP events into your application's request type
2. **`headers`** - Adds custom headers based on rendered content
3. **`to_response`** - Generates the response content (HTML/JSON) from prepared request data
4. **`to_body`** - Converts renderer content to Lambda response format (typically unused in Lambda runtime)

### Content Types

Three content types are supported:

- **`Content::Html(String)`** - HTML responses with `text/html; charset=utf-8` content type
- **`Content::Json(serde_json::Value)`** - JSON responses with `application/json` content type
- **`Content::Raw { data, content_type }`** - Binary content with custom MIME types

### Automatic Gzip Compression

All responses are automatically compressed with gzip, reducing bandwidth and improving performance. The Lambda runtime handles this transparently.

### Error Handling

The `LambdaResponseProcessor` methods return `Result<_, lambda_runtime::Error>`, allowing you to handle errors gracefully. Returning `Ok(None)` from `to_response` indicates no response should be sent.

### Async Processing

All response processing is async, allowing you to make database queries, API calls, or other I/O operations without blocking:

```rust
async fn to_response(&self, data: T) -> Result<...> {
    let user = database.fetch_user(data).await?;
    Ok(Some((Content::Html(render_user(user)), None)))
}
```

## Testing the Example

### Manual Testing with curl

Test each endpoint:

```bash
# Test home page
curl -i http://localhost:9000/lambda-url/basic_lambda_app_example/

# Test about page
curl -i http://localhost:9000/lambda-url/basic_lambda_app_example/about

# Test health API
curl http://localhost:9000/lambda-url/basic_lambda_app_example/api/health | jq

# Test info API
curl http://localhost:9000/lambda-url/basic_lambda_app_example/api/info | jq

# Test 404 handling
curl -i http://localhost:9000/lambda-url/basic_lambda_app_example/nonexistent

# Verify gzip compression
curl -H "Accept-Encoding: gzip" --compressed -I http://localhost:9000/lambda-url/basic_lambda_app_example/
```

### Verify Response Headers

Check that custom headers are present:

```bash
curl -I http://localhost:9000/lambda-url/basic_lambda_app_example/
```

Expected headers:

```
HTTP/1.1 200 OK
content-type: text/html; charset=utf-8
content-encoding: gzip
x-powered-by: HyperChad
cache-control: public, max-age=300
```

### Load Testing

For production deployments, test Lambda scalability:

```bash
# Install Apache Bench
apt-get install apache2-utils

# Run load test
ab -n 1000 -c 10 https://your-lambda-url.amazonaws.com/
```

## Troubleshooting

### "error loading shared libraries" when running locally

**Problem:** The binary can't find required libraries.

**Solution:** Use `cargo lambda watch` which provides the correct Lambda runtime environment, or build with musl target:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### Lambda function times out

**Problem:** The function exceeds the configured timeout (default 3 seconds).

**Solution:** Increase Lambda timeout in your infrastructure configuration:

```yaml
# SAM template
Timeout: 30
```

### Cold start latency is high

**Problem:** Initial invocations are slow.

**Solution:**

- Reduce binary size by removing unused dependencies
- Enable Lambda provisioned concurrency for predictable traffic
- Use Lambda SnapStart (if available for your runtime)

### Responses are not compressed

**Problem:** Responses don't have gzip encoding.

**Solution:** Ensure the Lambda proxy integration is configured correctly in API Gateway, and verify the client sends `Accept-Encoding: gzip` header.

### JSON responses show as HTML in browser

**Problem:** Browser displays download prompt instead of rendering JSON.

**Solution:** This is expected behavior. JSON responses are meant for API clients. Use a browser extension like JSONView or test with curl/Postman.

## Related Examples

- **hyperchad/examples/details_summary** - Advanced HTML component patterns with HyperChad
- **hyperchad/examples/http_events** - HTTP event handling patterns
- **web_server/examples/simple_get** - Similar web server example using Actix instead of Lambda

For more information on HyperChad template DSL, see the `hyperchad_template` package documentation.
