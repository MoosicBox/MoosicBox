#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::significant_drop_tightening)]

//! Basic WebSocket Message Handler Example
//!
//! This example demonstrates the core functionality of `moosicbox_ws`:
//! - Implementing the `WebsocketSender` trait for message broadcasting
//! - Processing inbound WebSocket messages
//! - Handling connection lifecycle (connect/disconnect)
//! - Managing connection state

use async_trait::async_trait;
use moosicbox_ws::{
    WebsocketContext, WebsocketSendError, WebsocketSender, connect, models::InboundPayload,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simple in-memory WebSocket connection manager
///
/// In a real implementation, this would be backed by actual WebSocket connections
/// (e.g., using actix-web, axum, tokio-tungstenite, etc.)
struct SimpleWebsocketSender {
    /// Map of connection IDs to their message queues
    connections: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SimpleWebsocketSender {
    fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a new connection to the manager
    async fn add_connection(&self, connection_id: String) {
        let mut connections = self.connections.write().await;
        connections.insert(connection_id, Vec::new());
    }

    /// Remove a connection from the manager
    async fn remove_connection(&self, connection_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(connection_id);
    }

    /// Get all messages sent to a specific connection
    async fn get_messages(&self, connection_id: &str) -> Vec<String> {
        let connections = self.connections.read().await;
        connections.get(connection_id).cloned().unwrap_or_default()
    }

    /// Get count of active connections
    async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }
}

#[async_trait]
impl WebsocketSender for SimpleWebsocketSender {
    /// Send a message to a specific connection
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let mut connections = self.connections.write().await;

        connections.get_mut(connection_id).map_or_else(
            || {
                Err(WebsocketSendError::Unknown(format!(
                    "Connection {connection_id} not found"
                )))
            },
            |messages| {
                messages.push(data.to_string());
                println!("✉️  Sent to [{connection_id}]: {data}");
                Ok(())
            },
        )
    }

    /// Broadcast a message to all connections
    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        let mut connections = self.connections.write().await;

        for (connection_id, messages) in connections.iter_mut() {
            messages.push(data.to_string());
            println!("📢 Broadcast to [{connection_id}]: {data}");
        }

        Ok(())
    }

    /// Broadcast a message to all connections except the specified one
    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        let mut connections = self.connections.write().await;

        for (conn_id, messages) in connections.iter_mut() {
            if conn_id != connection_id {
                messages.push(data.to_string());
                println!("📢 Broadcast to [{conn_id}] (except {connection_id}): {data}");
            }
        }

        Ok(())
    }

    /// Send ping to all connections to keep them alive
    async fn ping(&self) -> Result<(), WebsocketSendError> {
        let count = {
            let connections = self.connections.read().await;
            connections.len()
        };
        println!("🏓 Ping sent to {count} connection(s)");
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 MoosicBox WebSocket Message Handler Example");
    println!("================================================\n");

    // Step 1: Create a WebSocket sender implementation
    println!("Step 1: Creating WebSocket sender...");
    let sender = SimpleWebsocketSender::new();
    println!("✅ WebSocket sender created\n");

    // Step 2: Simulate client connections
    println!("Step 2: Simulating client connections...");

    // Add first connection
    let connection_id_1 = "client-abc-123".to_string();
    sender.add_connection(connection_id_1.clone()).await;

    let context_1 = WebsocketContext {
        connection_id: connection_id_1.clone(),
        profile: Some("default".to_string()),
        player_actions: vec![],
    };

    let response = connect(&sender, &context_1);
    println!(
        "✅ Client 1 connected: {} (Status: {})",
        connection_id_1, response.status_code
    );

    // Add second connection
    let connection_id_2 = "client-xyz-456".to_string();
    sender.add_connection(connection_id_2.clone()).await;

    let context_2 = WebsocketContext {
        connection_id: connection_id_2.clone(),
        profile: Some("default".to_string()),
        player_actions: vec![],
    };

    let response = connect(&sender, &context_2);
    println!(
        "✅ Client 2 connected: {} (Status: {})",
        connection_id_2, response.status_code
    );
    println!(
        "📊 Total active connections: {}\n",
        sender.connection_count().await
    );

    // Step 3: Send messages to specific connections
    println!("Step 3: Sending targeted messages...");
    sender
        .send(
            &connection_id_1,
            r#"{"type":"CONNECTION_ID","connectionId":"client-abc-123"}"#,
        )
        .await?;
    println!();

    // Step 4: Broadcast messages to all connections
    println!("Step 4: Broadcasting to all connections...");
    sender
        .send_all(r#"{"type":"SESSIONS","payload":[]}"#)
        .await?;
    println!();

    // Step 5: Broadcast to all except one connection
    println!("Step 5: Broadcasting to all except client 1...");
    sender
        .send_all_except(
            &connection_id_1,
            r#"{"type":"SESSION_UPDATED","payload":{"sessionId":1,"playing":true}}"#,
        )
        .await?;
    println!();

    // Step 6: Send ping to maintain connections
    println!("Step 6: Sending ping to all connections...");
    sender.ping().await?;
    println!();

    // Step 7: Process an inbound message
    println!("Step 7: Processing inbound messages...");
    let ping_message = serde_json::json!({
        "type": "PING"
    });

    if let Ok(payload) = serde_json::from_value::<InboundPayload>(ping_message) {
        println!("📨 Received inbound message: {payload:?}");
        // In a real implementation, you would call process_message here with a database
        // process_message(&config_db, body, context, &sender).await?;
    }
    println!();

    // Step 8: Display message queues
    println!("Step 8: Message summary...");
    println!("Messages received by client 1:");
    for (i, msg) in sender
        .get_messages(&connection_id_1)
        .await
        .iter()
        .enumerate()
    {
        println!("  {}. {}", i + 1, msg);
    }
    println!();

    println!("Messages received by client 2:");
    for (i, msg) in sender
        .get_messages(&connection_id_2)
        .await
        .iter()
        .enumerate()
    {
        println!("  {}. {}", i + 1, msg);
    }
    println!();

    // Step 9: Simulate disconnection
    println!("Step 9: Disconnecting clients...");
    sender.remove_connection(&connection_id_1).await;
    println!("✅ Client 1 disconnected");

    sender.remove_connection(&connection_id_2).await;
    println!("✅ Client 2 disconnected");

    println!(
        "📊 Total active connections: {}\n",
        sender.connection_count().await
    );

    println!("================================================");
    println!("✨ Example completed successfully!");
    println!("\n💡 Key Takeaways:");
    println!("   • Implement WebsocketSender trait for your WebSocket framework");
    println!("   • Use WebsocketContext to track connection metadata");
    println!("   • Call connect() when clients connect");
    println!("   • Use send(), send_all(), and send_all_except() to broadcast messages");
    println!("   • Send regular pings to keep connections alive");
    println!("   • In production, integrate with a database and call process_message()");

    Ok(())
}
