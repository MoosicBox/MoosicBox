//! Basic Priority Example
//!
//! This example demonstrates how to use the prioritized channel utilities
//! from `moosicbox_channel_utils` to send and receive messages with priority ordering.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use futures::StreamExt;
use moosicbox_channel_utils::{MoosicBoxSender, futures_channel::unbounded};

/// Represents a task with an associated priority level
#[derive(Debug, Clone)]
struct Task {
    id: u32,
    name: String,
    priority: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Channel Utils - Basic Priority Example ===\n");

    // Example 1: Simple string messages with priority
    println!("Example 1: String messages with priority keywords\n");
    example_string_priority().await?;

    println!("\n---\n");

    // Example 2: Structured tasks with explicit priority field
    println!("Example 2: Task queue with explicit priorities\n");
    example_task_priority().await?;

    println!("\n---\n");

    // Example 3: Without priority (FIFO order)
    println!("Example 3: Regular channel without priority (FIFO)\n");
    example_no_priority().await?;

    Ok(())
}

/// Demonstrates priority-based message ordering using string content
async fn example_string_priority() -> Result<(), Box<dyn std::error::Error>> {
    // Create an unbounded prioritized channel
    let (tx, mut rx) = unbounded::<String>();

    // Configure priority function based on message content
    // Higher return values = higher priority
    let tx = tx.with_priority(|msg: &String| {
        if msg.contains("CRITICAL") {
            100
        } else if msg.contains("HIGH") {
            50
        } else if msg.contains("MEDIUM") {
            25
        } else {
            1
        }
    });

    // Send messages in random order
    println!("Sending messages:");
    let messages = vec![
        "LOW: Regular system update",
        "MEDIUM: User notification pending",
        "LOW: Background task completed",
        "CRITICAL: Security alert detected!",
        "HIGH: Database connection lost",
        "MEDIUM: Cache invalidation needed",
        "CRITICAL: Memory threshold exceeded!",
    ];

    let message_count = messages.len();
    for msg in messages {
        println!("  Sent: {msg}");
        tx.send(msg.to_string())?;
    }

    // Drop the sender so the receiver can finish
    drop(tx);

    // Receive messages in priority order
    // Note: We collect a specific number of messages since the receiver
    // holds an internal sender clone that keeps the channel open
    println!("\nReceiving messages in priority order:");
    for _ in 0..message_count {
        if let Some(message) = rx.next().await {
            println!("  Received: {message}");
        }
    }

    Ok(())
}

/// Demonstrates priority-based task processing using structured data
async fn example_task_priority() -> Result<(), Box<dyn std::error::Error>> {
    // Create channel for Task messages
    let (tx, mut rx) = unbounded::<Task>();

    // Configure priority based on the task's priority field
    let tx = tx.with_priority(|task: &Task| task.priority as usize);

    // Send tasks with different priorities
    println!("Queuing tasks:");
    let tasks = vec![
        Task {
            id: 1,
            name: "Backup database".to_string(),
            priority: 3,
        },
        Task {
            id: 2,
            name: "Process payments".to_string(),
            priority: 10,
        },
        Task {
            id: 3,
            name: "Send emails".to_string(),
            priority: 1,
        },
        Task {
            id: 4,
            name: "Handle user request".to_string(),
            priority: 8,
        },
        Task {
            id: 5,
            name: "Update cache".to_string(),
            priority: 2,
        },
        Task {
            id: 6,
            name: "Emergency shutdown".to_string(),
            priority: 10,
        },
    ];

    let task_count = tasks.len();
    for task in tasks {
        println!(
            "  Queued: Task {} - '{}' (priority: {})",
            task.id, task.name, task.priority
        );
        tx.send(task)?;
    }

    // Drop sender
    drop(tx);

    // Process tasks in priority order
    // Note: We collect a specific number of tasks since the receiver
    // holds an internal sender clone that keeps the channel open
    println!("\nProcessing tasks in priority order:");
    for _ in 0..task_count {
        if let Some(task) = rx.next().await {
            println!(
                "  Processing: Task {} - '{}' (priority: {})",
                task.id, task.name, task.priority
            );
        }
    }

    Ok(())
}

/// Demonstrates regular FIFO behavior without priority function
async fn example_no_priority() -> Result<(), Box<dyn std::error::Error>> {
    // Create channel without configuring priority
    let (tx, mut rx) = unbounded::<String>();
    // Note: No .with_priority() call means FIFO order

    // Send messages
    println!("Sending messages:");
    let messages = vec![
        "First message",
        "Second message",
        "Third message",
        "Fourth message",
    ];

    let message_count = messages.len();
    for msg in messages {
        println!("  Sent: {msg}");
        tx.send(msg.to_string())?;
    }

    drop(tx);

    // Receive in FIFO order
    // Note: We collect a specific number of messages since the receiver
    // holds an internal sender clone that keeps the channel open
    println!("\nReceiving messages in FIFO order:");
    for _ in 0..message_count {
        if let Some(message) = rx.next().await {
            println!("  Received: {message}");
        }
    }

    Ok(())
}
