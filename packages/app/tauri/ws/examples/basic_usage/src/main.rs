#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_app_ws`
//!
//! This example demonstrates:
//! - Creating a WebSocket client
//! - Connecting to a WebSocket server
//! - Sending and receiving messages
//! - Handling connection lifecycle
//! - Graceful shutdown with cancellation tokens

use moosicbox_app_ws::{WebsocketSender, WsClient, WsMessage};
use switchy_async::util::CancellationToken;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see connection status
    env_logger::init();

    println!("=== MoosicBox WebSocket Client Example ===\n");

    // Note: This example connects to ws://localhost:8080
    // You can start a test WebSocket server using:
    // - websocat -s 8080 (if you have websocat installed)
    // - or any other WebSocket server on port 8080
    let websocket_url = "ws://localhost:8080".to_string();

    // Create a WebSocket client and handle
    // The client manages the connection, while the handle allows you to send messages
    let (client, handle) = WsClient::new(websocket_url);

    // Create a cancellation token for graceful shutdown
    let cancellation_token = CancellationToken::new();

    // Clone tokens for different tasks
    let shutdown_token = cancellation_token.clone();
    let client_token = cancellation_token.clone();

    // Create a client with the cancellation token
    let client = client.with_cancellation_token(client_token);

    // Create a channel to receive messages from the WebSocket server
    let (tx, mut rx) = mpsc::channel::<WsMessage>(100);

    // Spawn a task to handle incoming messages
    let receiver_task = tokio::spawn(async move {
        println!("Message receiver started\n");

        while let Some(msg) = rx.recv().await {
            match msg {
                WsMessage::TextMessage(text) => {
                    println!("📥 Received text: {text}");
                }
                WsMessage::Message(bytes) => {
                    println!("📥 Received binary: {} bytes", bytes.len());
                }
                WsMessage::Ping => {
                    println!("🏓 Received ping from server");
                }
            }
        }

        println!("\nMessage receiver stopped");
    });

    // Clone the handle for the sender task
    let sender_handle = handle.clone();

    // Spawn a task to send messages periodically
    let sender_task = tokio::spawn(async move {
        // Wait a moment for the connection to establish
        sleep(Duration::from_secs(2)).await;

        println!("Starting to send messages...\n");

        for i in 1..=5 {
            let message = format!("Hello from client, message #{i}");
            println!("📤 Sending: {message}");

            if let Err(e) = sender_handle.send(&message).await {
                eprintln!("❌ Failed to send message: {e}");
                break;
            }

            // Send a ping
            if let Err(e) = sender_handle.ping().await {
                eprintln!("❌ Failed to send ping: {e}");
                break;
            }

            // Wait before sending the next message
            sleep(Duration::from_secs(3)).await;
        }

        println!("\nFinished sending messages");
    });

    // Start the WebSocket connection
    // This will automatically reconnect if the connection drops
    let connection_task = tokio::spawn(async move {
        let result = client
            .start(
                None,                  // client_id (optional)
                None,                  // signature_token (optional)
                "default".to_string(), // profile name
                || {
                    // Callback executed when connection is established
                    println!("✅ WebSocket connected!\n");
                },
                tx, // Channel to send received messages
            )
            .await;

        if let Err(e) = result {
            eprintln!("❌ WebSocket connection error: {e:?}");
        }
    });

    // Spawn a task to handle shutdown after a timeout
    tokio::spawn(async move {
        // Run for 20 seconds, then shut down
        sleep(Duration::from_secs(20)).await;

        println!("\n⏱️  Timeout reached, shutting down...");
        shutdown_token.cancel();
    });

    // Wait for the sender task to complete
    let _ = sender_task.await;

    // Close the WebSocket connection gracefully
    println!("\n🛑 Closing WebSocket connection...");
    handle.close();

    // Wait for the connection task to finish
    let _ = connection_task.await;

    // Wait for the receiver task to finish
    let _ = receiver_task.await;

    println!("\n✨ Example completed successfully!");

    Ok(())
}
