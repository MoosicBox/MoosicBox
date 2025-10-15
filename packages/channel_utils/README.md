# MoosicBox Channel Utils

Simple channel utilities library for the MoosicBox ecosystem, providing basic async channel traits and prioritized message passing for futures-channel integration.

## Features

- **Channel Traits**: Basic traits for channel senders
- **Prioritized Channels**: Unbounded channels with message priority support
- **Futures Integration**: Utilities for futures-channel mpsc channels
- **Message Buffering**: Internal buffering for priority-based message ordering

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_channel_utils = "0.1.4"

# Enable futures-channel support (enabled by default)
moosicbox_channel_utils = { version = "0.1.4", features = ["futures-channel"] }
```

## Usage

### Basic Channel Sender Trait

The `MoosicBoxSender` trait provides a consistent interface for channel senders. The `PrioritizedSender` implements this trait:

```rust
use moosicbox_channel_utils::MoosicBoxSender;
use moosicbox_channel_utils::futures_channel::unbounded;

// PrioritizedSender implements MoosicBoxSender
let (tx, mut rx) = unbounded::<String>();
tx.send("Hello".to_string())?;  // Uses the MoosicBoxSender trait
```

### Prioritized Channels

```rust
use moosicbox_channel_utils::futures_channel::{unbounded, PrioritizedSender};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a prioritized channel
    let (tx, mut rx) = unbounded::<String>();

    // Add priority function (higher numbers = higher priority)
    let tx_with_priority = tx.with_priority(|msg: &String| {
        if msg.contains("URGENT") { 100 }
        else if msg.contains("HIGH") { 50 }
        else { 1 }
    });

    // Send messages with different priorities
    tx_with_priority.send("Normal message".to_string())?;
    tx_with_priority.send("HIGH priority message".to_string())?;
    tx_with_priority.send("Another normal message".to_string())?;
    tx_with_priority.send("URGENT message".to_string())?;

    // Messages will be received in priority order
    while let Some(message) = rx.next().await {
        println!("Received: {}", message);
    }
    // Output order will prioritize URGENT, then HIGH, then normal messages

    Ok(())
}
```

### Priority Examples

```rust
use moosicbox_channel_utils::futures_channel::unbounded;

#[derive(Debug)]
struct Task {
    id: u32,
    priority: u8,
    data: String,
}

async fn priority_task_queue() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = unbounded::<Task>();

    // Set up priority based on task priority field
    let prioritized_tx = tx.with_priority(|task: &Task| task.priority as usize);

    // Send tasks with different priorities
    prioritized_tx.send(Task { id: 1, priority: 1, data: "Low priority".to_string() })?;
    prioritized_tx.send(Task { id: 2, priority: 5, data: "High priority".to_string() })?;
    prioritized_tx.send(Task { id: 3, priority: 3, data: "Medium priority".to_string() })?;
    prioritized_tx.send(Task { id: 4, priority: 5, data: "Also high priority".to_string() })?;

    // Process tasks in priority order
    while let Some(task) = rx.next().await {
        println!("Processing task {}: {} (priority {})", task.id, task.data, task.priority);
    }

    Ok(())
}
```

### Internal Buffering

The prioritized sender maintains an internal buffer to sort messages by priority before sending them through the underlying channel. Messages are flushed from the buffer in priority order when the receiver processes items. This buffering mechanism ensures that higher-priority messages can "jump ahead" in the queue when they arrive.

## Core Types

### MoosicBoxSender<T, E>

Basic trait for channel senders with a consistent `send` method.

### PrioritizedSender<T>

An unbounded sender that buffers and sorts messages by priority before sending.

### PrioritizedReceiver<T>

A receiver that works with PrioritizedSender to process messages in priority order.

## Cargo Features

- **futures-channel** (default): Enables prioritized channel utilities for futures-channel mpsc

This library provides basic channel utilities focused on priority-based message ordering for async applications in the MoosicBox ecosystem.
