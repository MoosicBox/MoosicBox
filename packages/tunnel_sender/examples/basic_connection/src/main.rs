#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic tunnel connection example
//!
//! This example demonstrates how to establish a WebSocket tunnel connection
//! to a remote `MoosicBox` server and receive messages through the tunnel.

use moosicbox_tunnel_sender::{TunnelMessage, sender::TunnelSender};
use std::sync::Arc;
use switchy_database::{Database, config::ConfigDatabase, turso::TursoDatabase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see connection status and messages
    env_logger::init();

    println!("=== MoosicBox Tunnel Sender - Basic Connection Example ===\n");

    // Step 1: Configure connection parameters
    // In a real application, these would come from configuration or environment variables
    let host = std::env::var("TUNNEL_HOST")
        .unwrap_or_else(|_| "https://example.moosicbox.com".to_string());
    let ws_url = std::env::var("TUNNEL_WS_URL")
        .unwrap_or_else(|_| "wss://example.moosicbox.com/tunnel".to_string());
    let client_id =
        std::env::var("TUNNEL_CLIENT_ID").unwrap_or_else(|_| "demo-client-123".to_string());
    let access_token = std::env::var("TUNNEL_ACCESS_TOKEN")
        .unwrap_or_else(|_| "demo-access-token-456".to_string());

    println!("Configuration:");
    println!("  Host:      {host}");
    println!("  WS URL:    {ws_url}");
    println!("  Client ID: {client_id}");
    println!(
        "  Token:     {}...",
        &access_token[..access_token.len().min(10)]
    );
    println!();

    // Step 2: Create a configuration database
    // This stores tunnel configuration and state
    // For this example, we use an in-memory database
    println!("Initializing configuration database...");
    let db = TursoDatabase::new(":memory:").await?;
    let db_boxed: Box<dyn Database> = Box::new(db);
    let config_db = ConfigDatabase::from(Arc::new(db_boxed));

    println!("Creating tunnel sender...");

    // Step 3: Create the tunnel sender and handle
    // The sender manages the connection, while the handle allows control
    let (sender, handle) = TunnelSender::new(host, ws_url, client_id, access_token, config_db);

    println!("Tunnel sender created successfully");
    println!("Handle allows connection control (e.g., closing the tunnel)");
    println!();

    // Step 4: Start the tunnel connection
    // This returns a receiver for incoming messages
    println!("Starting tunnel connection...");
    println!("The tunnel will automatically:");
    println!("  - Fetch authentication signature token");
    println!("  - Establish WebSocket connection");
    println!("  - Handle reconnection on failures");
    println!("  - Send periodic ping messages");
    println!();

    let mut receiver = sender.start();

    println!("Tunnel started! Waiting for messages...");
    println!("Press Ctrl+C to stop");
    println!();

    // Step 5: Process incoming messages
    let mut message_count = 0;
    while let Some(message) = receiver.recv().await {
        message_count += 1;
        println!("Received message #{message_count}:");

        // Handle different message types
        match message {
            TunnelMessage::Text(text) => {
                println!("  Type: Text");
                println!("  Content: {text}");
            }
            TunnelMessage::Binary(bytes) => {
                println!("  Type: Binary");
                println!("  Size: {} bytes", bytes.len());
                println!("  Preview: {bytes:?}");
            }
            TunnelMessage::Ping(data) => {
                println!("  Type: Ping");
                println!("  Payload: {} bytes", data.len());
            }
            TunnelMessage::Pong(data) => {
                println!("  Type: Pong");
                println!("  Payload: {} bytes", data.len());
            }
            TunnelMessage::Close => {
                println!("  Type: Close");
                println!("Connection closed by server");
                break;
            }
            TunnelMessage::Frame(frame) => {
                println!("  Type: Raw Frame");
                println!("  Frame: {frame:?}");
            }
        }
        println!();

        // For demo purposes, limit to 10 messages
        // Remove this in real applications
        if message_count >= 10 {
            println!("Received 10 messages, closing for demo purposes");
            break;
        }
    }

    // Step 6: Clean up
    println!("Closing tunnel connection...");
    handle.close();
    println!("Tunnel closed successfully");
    println!("\nTotal messages received: {message_count}");

    Ok(())
}
