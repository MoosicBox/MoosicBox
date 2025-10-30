#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic AWS Lambda application example using `HyperChad` renderer.
//!
//! This example demonstrates how to create a serverless web application
//! that handles HTTP requests in AWS Lambda, renders HTML pages, and
//! serves JSON API responses.

use async_trait::async_trait;
use bytes::Bytes;
use hyperchad_renderer::ToRenderRunner;
use hyperchad_renderer_html_lambda::{
    Content, LambdaApp, LambdaResponseProcessor, lambda_runtime::Error,
};
use lambda_http::Request;
use std::sync::Arc;

/// Request processor for our Lambda application.
///
/// This struct implements the `LambdaResponseProcessor` trait to handle
/// incoming HTTP requests and generate appropriate responses.
#[derive(Clone)]
struct SimpleProcessor;

#[async_trait]
impl LambdaResponseProcessor<(String, String)> for SimpleProcessor {
    /// Prepares the incoming request by extracting the HTTP method and path.
    ///
    /// This method is called first to transform the raw Lambda HTTP request
    /// into a format suitable for our application logic.
    fn prepare_request(
        &self,
        req: Request,
        _body: Option<Arc<Bytes>>,
    ) -> Result<(String, String), Error> {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();

        println!("Processing request: {method} {path}");

        Ok((method, path))
    }

    /// Returns additional headers to include in the response.
    ///
    /// These headers are added based on the rendered content. Useful for
    /// adding cache control, security headers, or custom metadata.
    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        Some(vec![
            ("X-Powered-By".to_string(), "HyperChad".to_string()),
            (
                "Cache-Control".to_string(),
                "public, max-age=300".to_string(),
            ),
        ])
    }

    /// Generates the response content based on the request method and path.
    ///
    /// This is where the main routing logic lives. We match on the method
    /// and path to determine what content to return.
    async fn to_response(
        &self,
        (method, path): (String, String),
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, Error> {
        match (method.as_str(), path.as_str()) {
            // Home page - render HTML
            ("GET", "/") => {
                let html = render_home_page();
                Ok(Some((Content::Html(html), None)))
            }

            // About page - render HTML
            ("GET", "/about") => {
                let html = render_about_page();
                Ok(Some((Content::Html(html), None)))
            }

            // Health check API - return JSON
            ("GET", "/api/health") => {
                let json = serde_json::json!({
                    "status": "healthy",
                    "service": "hyperchad-lambda",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Ok(Some((Content::Json(json), None)))
            }

            // User info API - return JSON
            ("GET", "/api/info") => {
                let json = serde_json::json!({
                    "version": "1.0.0",
                    "renderer": "hyperchad_renderer_html_lambda",
                    "features": ["html", "json", "gzip-compression"]
                });
                Ok(Some((Content::Json(json), None)))
            }

            // 404 - route not found
            _ => {
                let html = render_404_page(&path);
                Ok(Some((Content::Html(html), None)))
            }
        }
    }

    /// Converts rendered content to response body format.
    ///
    /// Note: This method is not typically used in Lambda runtime, as the
    /// `to_response` method handles the full response generation.
    async fn to_body(
        &self,
        _content: hyperchad_renderer::Content,
        _data: (String, String),
    ) -> Result<Content, Error> {
        // Not used in this Lambda implementation
        Ok(Content::Html(String::new()))
    }
}

/// Renders the home page HTML.
fn render_home_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HyperChad Lambda Example</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 2rem;
            line-height: 1.6;
            color: #333;
        }
        h1 {
            color: #2c3e50;
            border-bottom: 3px solid #3498db;
            padding-bottom: 0.5rem;
        }
        nav {
            margin: 2rem 0;
            padding: 1rem;
            background: #f8f9fa;
            border-radius: 8px;
        }
        nav a {
            margin-right: 1rem;
            color: #3498db;
            text-decoration: none;
            font-weight: 500;
        }
        nav a:hover {
            text-decoration: underline;
        }
        .info-box {
            background: #e8f4f8;
            border-left: 4px solid #3498db;
            padding: 1rem;
            margin: 1rem 0;
        }
        ul {
            list-style-type: none;
            padding-left: 0;
        }
        li:before {
            content: "✓ ";
            color: #27ae60;
            font-weight: bold;
            margin-right: 0.5rem;
        }
    </style>
</head>
<body>
    <h1>Welcome to HyperChad on AWS Lambda!</h1>

    <p>
        This is a serverless web application powered by HyperChad's Lambda renderer.
        It demonstrates how to build fast, scalable web applications without managing servers.
    </p>

    <nav>
        <a href="/">Home</a>
        <a href="/about">About</a>
        <a href="/api/health">Health Check</a>
        <a href="/api/info">Info</a>
    </nav>

    <div class="info-box">
        <h2>Key Features</h2>
        <ul>
            <li>Automatic scaling based on demand</li>
            <li>Pay only for actual request processing time</li>
            <li>Built-in gzip compression for all responses</li>
            <li>Support for HTML and JSON responses</li>
            <li>Zero server management required</li>
            <li>Global edge deployment capability</li>
        </ul>
    </div>

    <p>
        <strong>Try it out:</strong> Navigate to the different routes using the links above
        to see how the Lambda function handles HTML pages and JSON API responses.
    </p>
</body>
</html>"#
        .to_string()
}

/// Renders the about page HTML.
fn render_about_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>About - HyperChad Lambda</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 2rem;
            line-height: 1.6;
            color: #333;
        }
        h1 {
            color: #2c3e50;
            border-bottom: 3px solid #3498db;
            padding-bottom: 0.5rem;
        }
        nav {
            margin: 2rem 0;
            padding: 1rem;
            background: #f8f9fa;
            border-radius: 8px;
        }
        nav a {
            color: #3498db;
            text-decoration: none;
            font-weight: 500;
        }
        nav a:hover {
            text-decoration: underline;
        }
        .tech-stack {
            background: #fff9e6;
            border: 1px solid #ffd700;
            border-radius: 8px;
            padding: 1rem;
            margin: 1rem 0;
        }
        code {
            background: #f4f4f4;
            padding: 0.2rem 0.4rem;
            border-radius: 3px;
            font-family: 'Courier New', monospace;
        }
    </style>
</head>
<body>
    <h1>About HyperChad Lambda Renderer</h1>

    <nav>
        <a href="/">← Back to Home</a>
    </nav>

    <p>
        HyperChad Lambda Renderer enables serverless deployment of HyperChad applications
        on AWS Lambda. It seamlessly integrates with AWS API Gateway and Application Load
        Balancers to handle HTTP requests.
    </p>

    <h2>How It Works</h2>
    <p>
        The renderer implements the <code>LambdaResponseProcessor</code> trait, which provides
        a clean interface for:
    </p>
    <ul>
        <li>Preparing incoming Lambda HTTP requests</li>
        <li>Generating HTML or JSON responses</li>
        <li>Adding custom headers</li>
        <li>Handling errors gracefully</li>
    </ul>

    <div class="tech-stack">
        <h3>Technology Stack</h3>
        <ul>
            <li><strong>hyperchad_renderer_html_lambda</strong> - Lambda runtime integration</li>
            <li><strong>lambda_http</strong> - AWS Lambda HTTP event handling</li>
            <li><strong>lambda_runtime</strong> - AWS Lambda runtime</li>
            <li><strong>serde_json</strong> - JSON serialization</li>
            <li><strong>flate2</strong> - gzip compression</li>
        </ul>
    </div>

    <h2>Use Cases</h2>
    <p>This renderer is perfect for:</p>
    <ul>
        <li>Serverless web applications</li>
        <li>REST APIs with server-side rendering</li>
        <li>Microservices architectures</li>
        <li>Event-driven web services</li>
    </ul>
</body>
</html>"#
        .to_string()
}

/// Renders a 404 error page.
fn render_404_page(path: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>404 - Page Not Found</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 2rem;
            line-height: 1.6;
            color: #333;
            text-align: center;
        }}
        h1 {{
            color: #e74c3c;
            font-size: 4rem;
            margin: 2rem 0 0.5rem 0;
        }}
        p {{
            font-size: 1.2rem;
            color: #666;
        }}
        code {{
            background: #f4f4f4;
            padding: 0.2rem 0.6rem;
            border-radius: 3px;
            font-family: 'Courier New', monospace;
        }}
        a {{
            display: inline-block;
            margin-top: 2rem;
            padding: 0.75rem 1.5rem;
            background: #3498db;
            color: white;
            text-decoration: none;
            border-radius: 5px;
            font-weight: 500;
        }}
        a:hover {{
            background: #2980b9;
        }}
    </style>
</head>
<body>
    <h1>404</h1>
    <p>The requested path <code>{path}</code> was not found.</p>
    <a href="/">Return to Home</a>
</body>
</html>"#
    )
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize logging
    env_logger::init();

    println!("Starting HyperChad Lambda application...");

    // Create the processor for handling requests
    let processor = SimpleProcessor;

    // Create the Lambda app
    let app = LambdaApp::new(processor);

    // Convert to a runner and execute
    let handle = hyperchad_renderer::Handle::current();
    let mut runner = app
        .to_runner(handle)
        .map_err(|e| Error::from(e.to_string()))?;

    println!("Lambda application ready - waiting for invocations");

    // Run the Lambda runtime
    runner.run().map_err(|e| Error::from(e.to_string()))?;

    Ok(())
}
