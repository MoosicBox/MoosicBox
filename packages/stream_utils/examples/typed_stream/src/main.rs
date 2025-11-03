#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Typed stream example demonstrating `TypedWriter` and `TypedStream`.
//!
//! This example shows how to use `TypedWriter` to broadcast strongly-typed
//! values to multiple `TypedStream` readers.

use futures::StreamExt;
use moosicbox_stream_utils::TypedWriter;

/// A custom event type to demonstrate typed streaming
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Event {
    UserLogin { username: String, timestamp: u64 },
    DataUpdate { key: String, value: i32 },
    SystemAlert { level: AlertLevel, message: String },
}

#[derive(Clone, Debug)]
enum AlertLevel {
    Info,
    Warning,
    Error,
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Typed Stream Example ===\n");

    // Create a TypedWriter for our Event type
    let writer = TypedWriter::<Event>::default();
    println!("Created TypedWriter for Event type\n");

    // Create multiple streams to receive events
    let mut event_logger = writer.stream();
    let mut metrics_collector = writer.stream();
    let mut alert_monitor = writer.stream();

    // Spawn task for event logging
    let logger_handle = tokio::spawn(async move {
        println!("[Logger] Starting to log events...");
        let mut count = 0;

        while let Some(event) = event_logger.next().await {
            count += 1;
            println!("[Logger] Event #{count}: {event:?}");
        }

        println!("[Logger] Finished logging {count} events\n");
    });

    // Spawn task for metrics collection
    let metrics_handle = tokio::spawn(async move {
        println!("[Metrics] Starting to collect metrics...");
        let mut login_count = 0;
        let mut update_count = 0;
        let mut alert_count = 0;

        while let Some(event) = metrics_collector.next().await {
            match event {
                Event::UserLogin { .. } => login_count += 1,
                Event::DataUpdate { .. } => update_count += 1,
                Event::SystemAlert { .. } => alert_count += 1,
            }
        }

        println!("[Metrics] Final counts:");
        println!("  - User logins: {login_count}");
        println!("  - Data updates: {update_count}");
        println!("  - System alerts: {alert_count}\n");
    });

    // Spawn task for alert monitoring (only cares about alerts)
    let alert_handle = tokio::spawn(async move {
        println!("[Alert Monitor] Starting to monitor system alerts...");
        let mut error_count = 0;

        while let Some(event) = alert_monitor.next().await {
            if let Event::SystemAlert { level, message } = event {
                match level {
                    AlertLevel::Error => {
                        error_count += 1;
                        eprintln!("[Alert Monitor] ERROR: {message}");
                    }
                    AlertLevel::Warning => {
                        println!("[Alert Monitor] WARNING: {message}");
                    }
                    AlertLevel::Info => {
                        println!("[Alert Monitor] INFO: {message}");
                    }
                }
            }
        }

        println!("[Alert Monitor] Detected {error_count} errors\n");
    });

    // Give tasks a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Write various events to the writer
    println!("[Writer] Broadcasting events...\n");

    writer.write(Event::UserLogin {
        username: "alice".to_string(),
        timestamp: 1_234_567_890,
    });

    writer.write(Event::SystemAlert {
        level: AlertLevel::Info,
        message: "System started successfully".to_string(),
    });

    writer.write(Event::DataUpdate {
        key: "temperature".to_string(),
        value: 72,
    });

    writer.write(Event::UserLogin {
        username: "bob".to_string(),
        timestamp: 1_234_567_900,
    });

    writer.write(Event::SystemAlert {
        level: AlertLevel::Warning,
        message: "High memory usage detected".to_string(),
    });

    writer.write(Event::DataUpdate {
        key: "humidity".to_string(),
        value: 65,
    });

    writer.write(Event::UserLogin {
        username: "charlie".to_string(),
        timestamp: 1_234_567_910,
    });

    writer.write(Event::SystemAlert {
        level: AlertLevel::Error,
        message: "Database connection lost".to_string(),
    });

    writer.write(Event::DataUpdate {
        key: "pressure".to_string(),
        value: 1013,
    });

    // Drop the writer to close all streams
    println!("\n[Writer] Dropping writer to close all streams...\n");
    drop(writer);

    // Wait for all tasks to complete
    logger_handle.await?;
    metrics_handle.await?;
    alert_handle.await?;

    println!("=== Example Complete ===");
    println!("\nKey takeaways:");
    println!("- TypedWriter broadcasts strongly-typed values to multiple readers");
    println!("- Each stream can process events differently based on their purpose");
    println!("- Type safety prevents sending wrong data types to streams");
    println!("- Values must implement Clone for broadcasting");
    println!("- Dropping the writer closes all connected streams");

    Ok(())
}
