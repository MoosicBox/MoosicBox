#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

//! Example demonstrating typed value broadcasting with `TypedWriter` and `TypedStream`.
//!
//! This example shows how to use `TypedWriter<T>` and `TypedStream<T>` to broadcast
//! strongly-typed values (not just bytes) to multiple readers simultaneously.

use futures::StreamExt;
use moosicbox_stream_utils::TypedWriter;

/// A custom event type to demonstrate typed broadcasting
#[derive(Debug, Clone, PartialEq)]
enum Event {
    UserJoined { username: String, user_id: u64 },
    MessageSent { from: String, message: String },
    UserLeft { username: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Typed Value Broadcasting Example ===\n");

    // Example 1: Broadcasting simple strings
    println!("--- Example 1: String Broadcasting ---\n");
    broadcast_strings().await?;

    println!("\n--- Example 2: Custom Event Broadcasting ---\n");
    broadcast_events().await?;

    Ok(())
}

async fn broadcast_strings() -> Result<(), Box<dyn std::error::Error>> {
    // Create a TypedWriter for String values
    let writer = TypedWriter::<String>::default();
    println!("Created TypedWriter<String>");

    // Create multiple streams
    let stream1 = writer.stream();
    let stream2 = writer.stream();
    println!("Created 2 streams\n");

    // Spawn tasks to read from streams
    let handle1 = tokio::spawn(async move {
        println!("Stream 1: Starting to read strings...");
        let mut messages = Vec::new();
        let mut stream = stream1;

        while let Some(msg) = stream.next().await {
            println!("Stream 1: Received: {msg:?}");
            messages.push(msg);
        }

        println!("Stream 1: Completed with {} messages", messages.len());
        messages
    });

    let handle2 = tokio::spawn(async move {
        println!("Stream 2: Starting to read strings...");
        let mut messages = Vec::new();
        let mut stream = stream2;

        while let Some(msg) = stream.next().await {
            println!("Stream 2: Received: {msg:?}");
            messages.push(msg);
        }

        println!("Stream 2: Completed with {} messages", messages.len());
        messages
    });

    // Give tasks time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Write typed values - they're broadcast to all streams
    println!("Writing strings to the TypedWriter...");
    writer.write("Hello from TypedWriter!".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    writer.write("This is message 2".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    writer.write("Final message".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Drop the writer to signal completion
    println!("Dropping the writer to signal completion...\n");
    drop(writer);

    // Wait for results
    let result1 = handle1.await?;
    let result2 = handle2.await?;

    // Verify both streams received the same data
    assert_eq!(result1.len(), 3);
    assert_eq!(result2.len(), 3);
    assert_eq!(result1, result2);

    println!("✓ Both streams received identical data!");

    Ok(())
}

async fn broadcast_events() -> Result<(), Box<dyn std::error::Error>> {
    // Create a TypedWriter for custom Event enum
    let writer = TypedWriter::<Event>::default();
    println!("Created TypedWriter<Event>");

    // Create streams for different consumers
    let logger_stream = writer.stream();
    let analytics_stream = writer.stream();
    let notifier_stream = writer.stream();
    println!("Created 3 streams (logger, analytics, notifier)\n");

    // Logger task - logs all events
    let logger_handle = tokio::spawn(async move {
        println!("[Logger] Starting event logging...");
        let mut event_count = 0;
        let mut stream = logger_stream;

        while let Some(event) = stream.next().await {
            event_count += 1;
            println!("[Logger] Event #{event_count}: {event:?}");
        }

        println!("[Logger] Finished logging {event_count} events");
        event_count
    });

    // Analytics task - counts event types
    let analytics_handle = tokio::spawn(async move {
        println!("[Analytics] Starting event analysis...");
        let mut joins = 0;
        let mut messages = 0;
        let mut leaves = 0;
        let mut stream = analytics_stream;

        while let Some(event) = stream.next().await {
            match event {
                Event::UserJoined { .. } => joins += 1,
                Event::MessageSent { .. } => messages += 1,
                Event::UserLeft { .. } => leaves += 1,
            }
        }

        println!("[Analytics] Summary:");
        println!("  - User joins: {joins}");
        println!("  - Messages: {messages}");
        println!("  - User leaves: {leaves}");

        (joins, messages, leaves)
    });

    // Notifier task - sends notifications for specific events
    let notifier_handle = tokio::spawn(async move {
        println!("[Notifier] Starting notification service...");
        let mut notifications_sent = 0;
        let mut stream = notifier_stream;

        while let Some(event) = stream.next().await {
            match event {
                Event::UserJoined { ref username, .. } => {
                    println!("[Notifier] 📢 Welcome {username}!");
                    notifications_sent += 1;
                }
                Event::UserLeft { ref username } => {
                    println!("[Notifier] 👋 Goodbye {username}!");
                    notifications_sent += 1;
                }
                Event::MessageSent { .. } => {
                    // Don't notify on every message
                }
            }
        }

        println!("[Notifier] Sent {notifications_sent} notifications");
        notifications_sent
    });

    // Give tasks time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Simulate a sequence of events
    println!("Broadcasting events...\n");

    writer.write(Event::UserJoined {
        username: "Alice".to_string(),
        user_id: 1,
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::UserJoined {
        username: "Bob".to_string(),
        user_id: 2,
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::MessageSent {
        from: "Alice".to_string(),
        message: "Hello everyone!".to_string(),
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::MessageSent {
        from: "Bob".to_string(),
        message: "Hi Alice!".to_string(),
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::UserJoined {
        username: "Charlie".to_string(),
        user_id: 3,
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::MessageSent {
        from: "Charlie".to_string(),
        message: "Hey folks!".to_string(),
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write(Event::UserLeft {
        username: "Bob".to_string(),
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Drop writer to signal completion
    println!("\nDropping writer to complete streams...\n");
    drop(writer);

    // Wait for all tasks to complete
    let event_count = logger_handle.await?;
    let (joins, messages, leaves) = analytics_handle.await?;
    let notifications = notifier_handle.await?;

    // Verify results
    println!("\n=== Results ===");
    println!("Total events processed: {event_count}");
    println!("Events by type: {joins} joins, {messages} messages, {leaves} leaves");
    println!("Notifications sent: {notifications}");

    assert_eq!(event_count, 7);
    assert_eq!(joins, 3);
    assert_eq!(messages, 3);
    assert_eq!(leaves, 1);
    assert_eq!(notifications, 4); // 3 joins + 1 leave

    println!("\n✓ All event consumers processed events correctly!");

    Ok(())
}
