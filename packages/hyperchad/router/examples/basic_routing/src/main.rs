#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::iter_on_single_items)]
#![allow(clippy::case_sensitive_file_extension_comparisons)]
#![allow(clippy::format_push_string)]

//! Basic routing example demonstrating core router patterns.
//!
//! This example shows how to:
//! - Create a router with multiple routes
//! - Match different route patterns (exact, multiple, prefix)
//! - Handle requests with different HTTP methods
//! - Parse JSON request bodies
//! - Access query parameters and headers
//! - Return different content types

use bytes::Bytes;
use hyperchad_renderer::Content;
use hyperchad_router::{RoutePath, RouteRequest, Router};
use serde::{Deserialize, Serialize};
use switchy::http::models::Method;
use switchy_async::runtime::Runtime;

/// User data structure for JSON parsing
#[derive(Debug, Deserialize, Serialize)]
struct User {
    name: String,
    email: String,
}

/// API response structure
#[derive(Debug, Serialize)]
struct ApiResponse {
    message: String,
    status: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== HyperChad Router - Basic Routing Example ===\n");

    // Create a new async runtime
    let runtime = Runtime::new();

    runtime.block_on(async {
        // Create router with various route patterns
        let router = create_router();

        println!("Router created with the following routes:");
        println!("  - GET /             (home page)");
        println!("  - GET /about        (about page)");
        println!("  - GET,POST /api/users (API endpoint with JSON)");
        println!("  - GET /api/v1 or /api/v2 (multiple literal routes)");
        println!("  - GET /static/*     (prefix route for static files)");
        println!("  - GET /query        (query parameter demo)");
        println!("  - GET /headers      (header access demo)\n");

        // Example 1: Navigate to home page
        println!("1. Navigating to home page (/)...");
        match router.navigate("/").await {
            Ok(Some(_content)) => println!("   Result: Got content successfully\n"),
            Ok(None) => println!("   Result: No content returned\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 2: Navigate to about page
        println!("2. Navigating to about page (/about)...");
        match router.navigate("/about").await {
            Ok(Some(_)) => println!("   Result: Got content successfully\n"),
            Ok(None) => println!("   Result: No content returned\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 3: GET request to API endpoint
        println!("3. Making GET request to /api/users...");
        let get_request = RouteRequest {
            path: "/api/users".to_string(),
            method: Method::Get,
            query: Default::default(),
            headers: Default::default(),
            cookies: Default::default(),
            info: Default::default(),
            body: None,
        };
        match router.navigate(get_request).await {
            Ok(Some(_)) => println!("   Result: Got user list\n"),
            Ok(None) => println!("   Result: No content returned\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 4: POST request with JSON body
        println!("4. Making POST request to /api/users with JSON body...");
        let user = User {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        };
        let json_body = serde_json::to_vec(&user)?;
        let post_request = RouteRequest {
            path: "/api/users".to_string(),
            method: Method::Post,
            query: Default::default(),
            headers: [("content-type".to_string(), "application/json".to_string())]
                .into_iter()
                .collect(),
            cookies: Default::default(),
            info: Default::default(),
            body: Some(std::sync::Arc::new(Bytes::from(json_body))),
        };
        match router.navigate(post_request).await {
            Ok(Some(_)) => println!("   Result: User created successfully\n"),
            Ok(None) => println!("   Result: No content returned\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 5: Multiple literal routes
        println!("5. Accessing API versions (multiple literal routes)...");
        for version in &["/api/v1", "/api/v2"] {
            println!("   Navigating to {version}...");
            match router.navigate(*version).await {
                Ok(Some(_)) => println!("   Result: Success\n"),
                Ok(None) => println!("   Result: No content\n"),
                Err(e) => println!("   Error: {e:?}\n"),
            }
        }

        // Example 6: Prefix route for static files
        println!("6. Accessing static files (prefix route)...");
        for file in &["/static/css/style.css", "/static/js/app.js"] {
            println!("   Navigating to {file}...");
            match router.navigate(*file).await {
                Ok(Some(_)) => println!("   Result: Got file content\n"),
                Ok(None) => println!("   Result: No content\n"),
                Err(e) => println!("   Error: {e:?}\n"),
            }
        }

        // Example 7: Query parameters
        println!("7. Accessing route with query parameters...");
        let query_request = RouteRequest {
            path: "/query".to_string(),
            method: Method::Get,
            query: [
                ("name".to_string(), "Bob".to_string()),
                ("age".to_string(), "30".to_string()),
            ]
            .into_iter()
            .collect(),
            headers: Default::default(),
            cookies: Default::default(),
            info: Default::default(),
            body: None,
        };
        match router.navigate(query_request).await {
            Ok(Some(_)) => println!("   Result: Processed query parameters\n"),
            Ok(None) => println!("   Result: No content\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 8: Headers access
        println!("8. Accessing route that reads headers...");
        let header_request = RouteRequest {
            path: "/headers".to_string(),
            method: Method::Get,
            query: Default::default(),
            headers: [
                (
                    "user-agent".to_string(),
                    "HyperChad-Example/1.0".to_string(),
                ),
                ("accept".to_string(), "application/json".to_string()),
            ]
            .into_iter()
            .collect(),
            cookies: Default::default(),
            info: Default::default(),
            body: None,
        };
        match router.navigate(header_request).await {
            Ok(Some(_)) => println!("   Result: Read headers successfully\n"),
            Ok(None) => println!("   Result: No content\n"),
            Err(e) => println!("   Error: {e:?}\n"),
        }

        // Example 9: Invalid route (should fail)
        println!("9. Attempting to navigate to non-existent route...");
        match router.navigate("/nonexistent").await {
            Ok(Some(_)) => println!("   Result: Unexpected success\n"),
            Ok(None) => println!("   Result: No content\n"),
            Err(e) => println!("   Error (expected): {e:?}\n"),
        }

        println!("=== Example completed successfully! ===");
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}

/// Create and configure the router with various route patterns
fn create_router() -> Router {
    Router::new()
        // Basic route: exact path match
        .with_route("/", |_req| async {
            "<h1>Welcome to HyperChad Router!</h1><p>This is the home page.</p>".to_string()
        })
        // Another simple route
        .with_route("/about", |_req| async {
            "<h1>About</h1><p>This demonstrates basic routing patterns.</p>".to_string()
        })
        // API route with different HTTP methods and JSON handling
        .with_route_result("/api/users", |req| async move {
            match req.method {
                Method::Get => {
                    // Return a list of users
                    let response = ApiResponse {
                        message: "User list retrieved".to_string(),
                        status: "success".to_string(),
                    };
                    let json = serde_json::to_string(&response)?;
                    Ok::<_, Box<dyn std::error::Error>>(json)
                }
                Method::Post => {
                    // Parse JSON body and create a user
                    let user: User = req.parse_body()?;
                    println!("   Creating user: {} ({})", user.name, user.email);
                    let response = ApiResponse {
                        message: format!("User {} created", user.name),
                        status: "success".to_string(),
                    };
                    let json = serde_json::to_string(&response)?;
                    Ok(json)
                }
                _ => {
                    // Method not allowed
                    let response = ApiResponse {
                        message: "Method not allowed".to_string(),
                        status: "error".to_string(),
                    };
                    let json = serde_json::to_string(&response)?;
                    Ok(json)
                }
            }
        })
        // Multiple literal routes: match either /api/v1 or /api/v2
        .with_route(&["/api/v1", "/api/v2"][..], |req| async move {
            format!("<h1>API Version</h1><p>You accessed: {}</p>", req.path)
        })
        // Prefix route: matches any path starting with /static/
        .with_route::<Content, Option<Content>, _>(
            RoutePath::LiteralPrefix("/static/".to_string()),
            |req| async move {
                // Extract the file path after /static/
                let file_path = req.path.strip_prefix("/static/").unwrap_or("");
                println!("   Serving static file: {file_path}");

                // Determine content type based on extension
                let content_type = if file_path.ends_with(".css") {
                    "text/css"
                } else if file_path.ends_with(".js") {
                    "application/javascript"
                } else {
                    "text/plain"
                };

                // Return raw content
                Some(Content::Raw {
                    data: Bytes::from(format!("/* Static file: {file_path} */")),
                    content_type: content_type.to_string(),
                })
            },
        )
        // Route demonstrating query parameter access
        .with_route("/query", |req| async move {
            let mut params = String::from("<h1>Query Parameters</h1><ul>");
            for (key, value) in &req.query {
                params.push_str(&format!("<li>{key} = {value}</li>"));
                println!("   Query param: {key} = {value}");
            }
            params.push_str("</ul>");
            params
        })
        // Route demonstrating header access
        .with_route("/headers", |req| async move {
            let mut headers_html = String::from("<h1>Request Headers</h1><ul>");
            for (key, value) in &req.headers {
                headers_html.push_str(&format!("<li>{key}: {value}</li>"));
                println!("   Header: {key}: {value}");
            }
            headers_html.push_str("</ul>");
            headers_html
        })
}
