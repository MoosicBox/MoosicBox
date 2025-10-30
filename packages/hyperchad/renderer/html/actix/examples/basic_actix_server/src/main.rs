#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Actix Server Example
//!
//! This example demonstrates a simple web server using the `HyperChad` Actix renderer.
//! It showcases basic routing, HTML rendering, and the `ActixResponseProcessor` pattern.

use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse};
use async_trait::async_trait;
use bytes::Bytes;
use hyperchad_renderer::{Content, RendererEvent, ToRenderRunner};
use hyperchad_renderer_html_actix::{ActixApp, ActixResponseProcessor};
use log::{error, info};

/// Simple request data structure
#[derive(Clone)]
struct SimpleRequest {
    path: String,
    method: String,
}

/// Our custom response processor that handles HTTP requests
#[derive(Clone)]
struct SimpleProcessor;

#[async_trait]
impl ActixResponseProcessor<SimpleRequest> for SimpleProcessor {
    /// Prepare request data from the HTTP request
    fn prepare_request(
        &self,
        req: HttpRequest,
        _body: Option<Arc<Bytes>>,
    ) -> Result<SimpleRequest, actix_web::Error> {
        info!("Preparing request for: {} {}", req.method(), req.path());

        Ok(SimpleRequest {
            path: req.path().to_string(),
            method: req.method().to_string(),
        })
    }

    /// Convert the request data into an HTTP response
    async fn to_response(&self, data: SimpleRequest) -> Result<HttpResponse, actix_web::Error> {
        info!("Generating response for: {} {}", data.method, data.path);

        // Generate HTML content based on the route
        let html_content = match data.path.as_str() {
            "/" => generate_home_page(),
            "/about" => generate_about_page(),
            "/contact" => generate_contact_page(),
            _ => generate_not_found_page(&data.path),
        };

        // Return the HTML response
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html_content))
    }

    /// Convert content to body bytes and content type (used for streaming updates)
    async fn to_body(
        &self,
        content: Content,
        _data: SimpleRequest,
    ) -> Result<(Bytes, String), actix_web::Error> {
        // Handle different content types
        let (bytes, content_type) = match content {
            Content::View(view) => {
                // In a real application, you would render the view to HTML
                // For this simple example, we'll use a basic representation
                let html = format!("{view:?}");
                (Bytes::from(html), "text/html; charset=utf-8".to_string())
            }
            Content::Raw { data, content_type } => (data, content_type),
        };
        Ok((bytes, content_type))
    }
}

/// Generate the home page HTML
#[allow(clippy::too_many_lines)]
fn generate_home_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HyperChad Actix Example - Home</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }
        .header {
            background-color: #2c3e50;
            color: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        .nav {
            margin-top: 15px;
        }
        .nav a {
            color: #3498db;
            text-decoration: none;
            margin-right: 15px;
            padding: 5px 10px;
            background-color: white;
            border-radius: 4px;
        }
        .nav a:hover {
            background-color: #ecf0f1;
        }
        .content {
            background-color: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .feature-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }
        .feature-card {
            padding: 20px;
            background-color: #ecf0f1;
            border-radius: 8px;
        }
        .footer {
            margin-top: 20px;
            text-align: center;
            color: #7f8c8d;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 HyperChad Actix Server</h1>
        <p>A simple web server example using Actix Web and HyperChad renderer</p>
        <div class="nav">
            <a href="/">Home</a>
            <a href="/about">About</a>
            <a href="/contact">Contact</a>
        </div>
    </div>

    <div class="content">
        <h2>Welcome!</h2>
        <p>
            This is a basic example of a web server built with <strong>HyperChad Actix Renderer</strong>.
            It demonstrates how to set up routes, process HTTP requests, and serve HTML content.
        </p>

        <h3>Key Features</h3>
        <div class="feature-grid">
            <div class="feature-card">
                <h4>🎯 Simple Routing</h4>
                <p>Easy-to-understand request handling with the ActixResponseProcessor trait</p>
            </div>
            <div class="feature-card">
                <h4>⚡ Actix Web</h4>
                <p>Built on top of the fast and reliable Actix Web framework</p>
            </div>
            <div class="feature-card">
                <h4>🎨 Clean HTML</h4>
                <p>Serve clean, semantic HTML with proper content types</p>
            </div>
        </div>

        <h3>Try It Out</h3>
        <ul>
            <li><a href="/">Visit the home page</a> (you're here!)</li>
            <li><a href="/about">Read about this example</a></li>
            <li><a href="/contact">Check out the contact page</a></li>
            <li><a href="/nonexistent">Try a non-existent route</a> (404 page)</li>
        </ul>
    </div>

    <div class="footer">
        <p>Built with ❤️ using HyperChad and Actix Web</p>
    </div>
</body>
</html>"#
        .to_string()
}

/// Generate the about page HTML
fn generate_about_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HyperChad Actix Example - About</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }
        .header {
            background-color: #2c3e50;
            color: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        .nav {
            margin-top: 15px;
        }
        .nav a {
            color: #3498db;
            text-decoration: none;
            margin-right: 15px;
            padding: 5px 10px;
            background-color: white;
            border-radius: 4px;
        }
        .content {
            background-color: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        code {
            background-color: #f8f8f8;
            padding: 2px 6px;
            border-radius: 3px;
            font-family: monospace;
        }
        pre {
            background-color: #f8f8f8;
            padding: 15px;
            border-radius: 5px;
            overflow-x: auto;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>📖 About This Example</h1>
        <div class="nav">
            <a href="/">Home</a>
            <a href="/about">About</a>
            <a href="/contact">Contact</a>
        </div>
    </div>

    <div class="content">
        <h2>What This Example Demonstrates</h2>
        <p>This example shows the core concepts of building a web server with HyperChad Actix Renderer:</p>

        <h3>1. ActixResponseProcessor Pattern</h3>
        <p>
            The <code>ActixResponseProcessor</code> trait provides three key methods:
        </p>
        <ul>
            <li><code>prepare_request()</code> - Extract data from HTTP requests</li>
            <li><code>to_response()</code> - Generate HTTP responses from request data</li>
            <li><code>to_body()</code> - Convert content to bytes for streaming</li>
        </ul>

        <h3>2. Simple Routing</h3>
        <p>
            Routes are handled in the <code>to_response()</code> method using a simple match statement
            on the request path. This makes it easy to add new routes and functionality.
        </p>

        <h3>3. HTML Content Generation</h3>
        <p>
            Each route generates its own HTML content with appropriate styling and structure.
            In a real application, you might use the HyperChad template system for more
            sophisticated component-based rendering.
        </p>

        <h3>4. Error Handling</h3>
        <p>
            The example includes a 404 page for non-existent routes, demonstrating proper
            error handling in the routing logic.
        </p>

        <h3>Running the Server</h3>
        <p>The server starts on <code>http://0.0.0.0:8343</code> by default, configurable via the <code>PORT</code> environment variable.</p>
    </div>
</body>
</html>"#
        .to_string()
}

/// Generate the contact page HTML
fn generate_contact_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HyperChad Actix Example - Contact</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }
        .header {
            background-color: #2c3e50;
            color: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        .nav {
            margin-top: 15px;
        }
        .nav a {
            color: #3498db;
            text-decoration: none;
            margin-right: 15px;
            padding: 5px 10px;
            background-color: white;
            border-radius: 4px;
        }
        .content {
            background-color: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .info-box {
            background-color: #e8f4f8;
            border-left: 4px solid #3498db;
            padding: 15px;
            margin: 20px 0;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>📧 Contact Information</h1>
        <div class="nav">
            <a href="/">Home</a>
            <a href="/about">About</a>
            <a href="/contact">Contact</a>
        </div>
    </div>

    <div class="content">
        <h2>Get in Touch</h2>
        <p>
            This is a demonstration contact page. In a real application, you might include
            a contact form, email addresses, or other ways to get in touch.
        </p>

        <div class="info-box">
            <h3>💡 Next Steps</h3>
            <p>To add form handling and POST request processing to this example, you would:</p>
            <ul>
                <li>Modify the <code>prepare_request()</code> method to parse form data from the body</li>
                <li>Add POST handling logic in <code>to_response()</code></li>
                <li>Use the <code>actions</code> feature for interactive form processing</li>
                <li>Add validation and error handling for user input</li>
            </ul>
        </div>

        <h3>Documentation</h3>
        <p>For more information about HyperChad Actix Renderer, check out:</p>
        <ul>
            <li>The package README for comprehensive documentation</li>
            <li>The source code in <code>packages/hyperchad/renderer/html/actix/</code></li>
            <li>Other examples in the HyperChad ecosystem</li>
        </ul>
    </div>
</body>
</html>"#
        .to_string()
}

/// Generate a 404 not found page
fn generate_not_found_page(path: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>404 - Page Not Found</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        .header {{
            background-color: #e74c3c;
            color: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
        }}
        .content {{
            background-color: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            text-align: center;
        }}
        code {{
            background-color: #f8f8f8;
            padding: 2px 6px;
            border-radius: 3px;
            font-family: monospace;
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>⚠️ 404 - Page Not Found</h1>
    </div>

    <div class="content">
        <h2>Oops! Page Not Found</h2>
        <p>The page <code>{}</code> does not exist.</p>
        <p><a href="/">← Return to Home</a></p>
    </div>
</body>
</html>"#,
        html_escape(path)
    )
}

/// Simple HTML escaping to prevent XSS
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting HyperChad Actix Server Example");

    // Create a channel for renderer events
    // In this simple example, we don't send any events, but the channel is required
    let (_tx, rx) = flume::unbounded::<RendererEvent>();

    // Create our response processor
    let processor = SimpleProcessor;

    // Create the Actix application
    let app = ActixApp::new(processor, rx);

    // Get the async runtime handle from switchy
    let handle = hyperchad_renderer::Handle::current();

    // Convert to a runner and execute
    let mut runner = app.to_runner(handle).map_err(|e| {
        error!("Failed to create runner: {e}");
        e
    })?;

    info!("Server is starting...");
    info!("Visit http://localhost:8343 to view the application");

    // Run the server (this blocks until the server shuts down)
    runner.run()?;

    info!("Server has shut down");
    Ok(())
}
