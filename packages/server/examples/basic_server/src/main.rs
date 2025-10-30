#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic `MoosicBox` Server Example
//!
//! This example demonstrates how to start a basic `MoosicBox` server instance with
//! default configuration. The server will:
//! - Initialize a `SQLite` database for configuration
//! - Start an HTTP server on localhost:8080
//! - Enable all API endpoints
//! - Support all audio formats (FLAC, MP3, AAC, OPUS)
//! - Listen for `WebSocket` connections for real-time updates

use moosicbox_config::AppType;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging to see server output
    // Parameters: filename for log file (optional), additional layers (optional)
    moosicbox_logging::init(Some("basic_server_example"), None)
        .expect("Failed to initialize logging");

    println!("Starting MoosicBox server...");
    println!("The server will be accessible at http://localhost:8080");
    println!("Press Ctrl+C to stop the server");

    // Start the server using the simplified run_basic function
    // Parameters:
    // - AppType::App: Indicates this is a standard application server
    // - "0.0.0.0": Bind to all network interfaces (accessible from other devices)
    // - 8080: Port number to listen on
    // - None: Use default number of Actix worker threads
    // - on_startup closure: Called when the server is ready
    let _handle = moosicbox_server::run_basic(AppType::App, "0.0.0.0", 8080, None, |handle| {
        println!("✓ Server started successfully!");
        println!();
        println!("Available endpoints:");
        println!("  - Health check: http://localhost:8080/health");
        println!("  - WebSocket: ws://localhost:8080/ws");
        println!("  - API docs: http://localhost:8080/openapi (if openapi feature enabled)");
        println!();
        println!("Server is now running and ready to accept connections...");

        // Return the server handle so the runtime can manage it
        handle
    })
    .await?;

    Ok(())
}
