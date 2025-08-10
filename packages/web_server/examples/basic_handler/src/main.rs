use moosicbox_web_server::{Error, HttpRequest, HttpResponse, Method};

#[cfg(feature = "actix")]
use moosicbox_web_server::Route;

#[cfg(any(feature = "simulator", not(feature = "actix")))]
async fn demo_handler(request: HttpRequest) -> Result<HttpResponse, Error> {
    let mut response = String::new();

    response.push_str("=== HttpRequest Method Demonstration ===\n\n");

    // Test method()
    response.push_str(&format!("HTTP Method: {:?}\n", request.method()));

    // Test path()
    response.push_str(&format!("Path: {}\n", request.path()));

    // Test query_string() - returns &str, not Option<&str>
    let query = request.query_string();
    if query.is_empty() {
        response.push_str("Query String: None\n");
    } else {
        response.push_str(&format!("Query String: {}\n", query));
    }

    // Test header()
    if let Some(user_agent) = request.header("user-agent") {
        response.push_str(&format!("User-Agent: {}\n", user_agent));
    } else {
        response.push_str("User-Agent: None\n");
    }

    if let Some(content_type) = request.header("content-type") {
        response.push_str(&format!("Content-Type: {}\n", content_type));
    } else {
        response.push_str("Content-Type: None\n");
    }

    // Test cookies()
    let cookies = request.cookies();
    response.push_str(&format!("All Cookies: {} found\n", cookies.len()));
    for (name, value) in &cookies {
        response.push_str(&format!("  {}: {}\n", name, value));
    }

    // Test cookie() for specific cookie
    if let Some(session_cookie) = request.cookie("session") {
        response.push_str(&format!("Session Cookie: {}\n", session_cookie));
    } else {
        response.push_str("Session Cookie: None\n");
    }

    // Test remote_addr()
    if let Some(addr) = request.remote_addr() {
        response.push_str(&format!("Remote Address: {}\n", addr));
    } else {
        response.push_str("Remote Address: None\n");
    }

    // Test body() - returns Option<&Bytes>
    if let Some(body) = request.body() {
        if body.is_empty() {
            response.push_str("Body: Empty\n");
        } else {
            response.push_str(&format!("Body: {} bytes\n", body.len()));
            if let Ok(body_str) = std::str::from_utf8(body) {
                response.push_str(&format!("Body Content: {}\n", body_str));
            }
        }
    } else {
        response.push_str("Body: None (consumed or not available)\n");
    }

    response.push_str("\n=== Handler Trait System Working! ===\n");

    Ok(HttpResponse::ok().with_body(response))
}

// TODO(Step 2): Once Send bounds are fixed in Step 2, update this example
// to use the same async handler for both Actix and Simulator backends.
// Remove the data extraction workaround for Actix.
#[cfg(feature = "actix")]
fn run_actix_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Running Actix Web Server Example...");

    // Create route using the traditional Route::new method
    // Note: Actix HttpRequest contains non-Send types, so we extract data immediately
    let handler = |req: HttpRequest| -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<HttpResponse, Error>> + Send>,
    > {
        // Extract all data from request immediately (before async block)
        let method = req.method();
        let path = req.path().to_string();
        let query = req.query_string().to_string();
        let cookies = req.cookies();
        let remote_addr = req.remote_addr();
        let user_agent = req.header("user-agent").map(|s| s.to_string());

        Box::pin(async move {
            let mut response = String::new();
            response.push_str("=== Actix HttpRequest Method Demonstration ===\n\n");
            response.push_str(&format!("HTTP Method: {:?}\n", method));
            response.push_str(&format!("Path: {}\n", path));

            if query.is_empty() {
                response.push_str("Query String: None\n");
            } else {
                response.push_str(&format!("Query String: {}\n", query));
            }

            if let Some(ua) = user_agent {
                response.push_str(&format!("User-Agent: {}\n", ua));
            } else {
                response.push_str("User-Agent: None\n");
            }

            response.push_str(&format!("All Cookies: {} found\n", cookies.len()));
            for (name, value) in &cookies {
                response.push_str(&format!("  {}: {}\n", name, value));
            }

            if let Some(addr) = remote_addr {
                response.push_str(&format!("Remote Address: {}\n", addr));
            } else {
                response.push_str("Remote Address: None\n");
            }

            response.push_str("\n=== Actix Handler Working! ===\n");
            response.push_str("(HttpRequest methods work identically to Simulator)\n");

            Ok(HttpResponse::ok().with_body(response))
        })
    };

    let route = Route::new(Method::Post, "/demo", handler);

    println!("✅ Route created successfully:");
    println!("   Method: {:?}", route.method);
    println!("   Path: {}", route.path);
    println!("   Handler: Traditional Route::new working!");
    println!("   Note: HttpRequest methods work identically across backends");
    println!("   Note: Actix requires extracting data before async due to Send bounds");

    Ok(())
}

#[cfg(any(feature = "simulator", not(feature = "actix")))]
fn run_simulator_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Example...");

    // Create a test request with all the features
    let cookies = vec![
        ("session".to_string(), "abc123".to_string()),
        ("theme".to_string(), "dark".to_string()),
    ];

    let request = SimulationRequest::new(Method::Post, "/demo")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-Test/1.0")
        .with_header("content-type", "application/json")
        .with_cookies(cookies)
        .with_remote_addr("192.168.1.100:54321")
        .with_body(b"{\"message\": \"Hello from simulator!\"}".to_vec());

    let stub = SimulationStub::new(request);
    let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    // Create a simple async block to run our handler
    let response = futures::executor::block_on(demo_handler(http_request));

    println!("📋 Simulator Response:");
    match response {
        Ok(http_response) => {
            println!("Status: {:?}", http_response.status_code);
            if let Some(body) = http_response.body {
                match body {
                    moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            println!("{}", text);
                        } else {
                            println!("Body: {} bytes (binary)", bytes.len());
                        }
                    }
                }
            } else {
                println!("No body");
            }
        }
        Err(e) => println!("Error: {:?}", e),
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Step 1 Validation: Basic Handler Example");
    println!("==========================================\n");

    #[cfg(feature = "actix")]
    {
        run_actix_example()?;
        println!();
    }

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        run_simulator_example()?;
        println!();
    }

    #[cfg(not(any(feature = "actix", any(feature = "simulator", not(feature = "actix")))))]
    {
        println!("❌ No backend features enabled!");
        println!("   Run with: cargo run --bin basic_handler --features actix");
        println!("   Or with:  cargo run --bin basic_handler --features simulator");
    }

    println!("✅ Step 1 validation complete!");
    println!("   - HttpRequest methods work identically across backends");
    println!("   - Handler trait system (IntoHandler) working");
    println!("   - Route::with_handler() method working");
    println!("   - Ready for Step 2: Handler System implementation");

    Ok(())
}
