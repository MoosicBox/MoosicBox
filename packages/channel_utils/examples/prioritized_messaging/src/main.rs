#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating prioritized message handling with `moosicbox_channel_utils`.
//!
//! This example shows how to use the `PrioritizedSender` and `PrioritizedReceiver` to
//! send and receive messages in priority order, ensuring high-priority messages are
//! processed before lower-priority ones.

use futures::StreamExt;
use moosicbox_channel_utils::{MoosicBoxSender, futures_channel::unbounded};

/// Represents a task with priority and data.
#[derive(Debug, Clone)]
struct Task {
    id: u32,
    priority: u8,
    description: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Prioritized Messaging Example ===\n");

    // Example 1: String messages with keyword-based priority
    println!("Example 1: Keyword-based priority");
    println!("-----------------------------------");
    string_priority_example().await?;

    println!("\n");

    // Example 2: Task struct with priority field
    println!("Example 2: Task priority field");
    println!("-------------------------------");
    task_priority_example().await?;

    println!("\n");

    // Example 3: Using the trait directly
    println!("Example 3: Using MoosicBoxSender trait");
    println!("---------------------------------------");
    trait_usage_example().await?;

    Ok(())
}

/// Demonstrates priority based on message content (keywords).
async fn string_priority_example() -> Result<(), Box<dyn std::error::Error>> {
    // Create an unbounded prioritized channel
    let (tx, mut rx) = unbounded::<String>();

    // Configure priority function based on message keywords
    // Higher numbers = higher priority
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

    // Send messages with different priorities in random order
    println!("Sending messages in the following order:");
    let messages = [
        "Normal message 1",
        "HIGH priority alert",
        "Normal message 2",
        "CRITICAL system failure",
        "MEDIUM priority warning",
        "Normal message 3",
    ];

    for (i, msg) in messages.iter().enumerate() {
        let num = i + 1;
        println!("  {num}. {msg}");
        tx.send((*msg).to_string())?;
    }

    // Drop the sender to close the channel
    drop(tx);

    // Receive messages in priority order
    println!("\nReceiving messages in priority order:");
    let mut count = 1;
    while let Some(message) = rx.next().await {
        println!("  {count}. {message}");
        count += 1;
    }

    Ok(())
}

/// Demonstrates priority based on struct fields.
async fn task_priority_example() -> Result<(), Box<dyn std::error::Error>> {
    // Create an unbounded prioritized channel for Task structs
    let (tx, mut rx) = unbounded::<Task>();

    // Configure priority based on task priority field
    let tx = tx.with_priority(|task: &Task| task.priority as usize);

    // Send tasks with different priorities
    println!("Sending tasks:");
    let tasks = vec![
        Task {
            id: 1,
            priority: 1,
            description: "Low priority background job".to_string(),
        },
        Task {
            id: 2,
            priority: 8,
            description: "High priority user request".to_string(),
        },
        Task {
            id: 3,
            priority: 3,
            description: "Medium priority maintenance".to_string(),
        },
        Task {
            id: 4,
            priority: 10,
            description: "Critical security update".to_string(),
        },
        Task {
            id: 5,
            priority: 5,
            description: "Medium-high priority sync".to_string(),
        },
    ];

    for task in &tasks {
        let id = task.id;
        let desc = &task.description;
        let prio = task.priority;
        println!("  Task {id}: {desc} (priority: {prio})");
        tx.send(task.clone())?;
    }

    // Drop the sender to close the channel
    drop(tx);

    // Process tasks in priority order
    println!("\nProcessing tasks in priority order:");
    let mut count = 1;
    while let Some(task) = rx.next().await {
        let id = task.id;
        let desc = &task.description;
        let prio = task.priority;
        println!("  {count}. Task {id}: {desc} (priority: {prio})");
        count += 1;
    }

    Ok(())
}

/// Demonstrates using the `MoosicBoxSender` trait directly.
async fn trait_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    // Create a prioritized channel
    let (tx, mut rx) = unbounded::<String>();

    // Configure priority
    let tx = tx.with_priority(|msg: &String| if msg.starts_with('!') { 100 } else { 1 });

    // The PrioritizedSender implements the MoosicBoxSender trait
    // This allows for consistent interface across different sender types
    send_via_trait(&tx, "Normal message".to_string())?;
    send_via_trait(&tx, "!Important message".to_string())?;
    send_via_trait(&tx, "Another normal message".to_string())?;

    // Drop the sender to close the channel
    drop(tx);

    // Receive messages
    println!("Messages sent via MoosicBoxSender trait:");
    while let Some(message) = rx.next().await {
        println!("  - {message}");
    }

    Ok(())
}

/// Generic function that accepts any type implementing `MoosicBoxSender`.
fn send_via_trait<S, E>(sender: &S, msg: String) -> Result<(), E>
where
    S: MoosicBoxSender<String, E>,
{
    sender.send(msg)
}
