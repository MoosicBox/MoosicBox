# Prioritized Messaging Example

This example demonstrates how to use `moosicbox_channel_utils` to create channels with prioritized message handling. Messages can be sent with different priorities, and the receiver will process higher-priority messages before lower-priority ones.

## Summary

This example shows three different approaches to using prioritized channels: keyword-based priority for strings, struct field-based priority for custom types, and using the `MoosicBoxSender` trait for generic sender implementations.

## What This Example Demonstrates

- Creating unbounded prioritized channels with `unbounded()`
- Configuring priority functions with `with_priority()`
- Sending messages through prioritized senders using the `MoosicBoxSender` trait
- Receiving messages in priority order using `PrioritizedReceiver`
- Using keyword-based priority for string messages
- Using struct field-based priority for custom types
- Generic trait-based sender functions

## Prerequisites

- Basic understanding of Rust async programming and the `tokio` runtime
- Familiarity with channels and message passing concepts
- Understanding of the `futures::Stream` trait

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/channel_utils/examples/prioritized_messaging/Cargo.toml
```

Or from the example directory:

```bash
cd packages/channel_utils/examples/prioritized_messaging
cargo run
```

## Expected Output

The example produces output showing three different scenarios:

```
=== Prioritized Messaging Example ===

Example 1: Keyword-based priority
-----------------------------------
Sending messages in the following order:
  1. Normal message 1
  2. HIGH priority alert
  3. Normal message 2
  4. CRITICAL system failure
  5. MEDIUM priority warning
  6. Normal message 3

Receiving messages in priority order:
  1. Normal message 1
  2. CRITICAL system failure
  3. HIGH priority alert
  4. MEDIUM priority warning
  5. Normal message 2
  6. Normal message 3

Note: The first message ("Normal message 1") is delivered immediately to prime the channel. Subsequent messages are then buffered and sorted by priority.

Example 2: Task priority field
-------------------------------
Sending tasks:
  Task 1: Low priority background job (priority: 1)
  Task 2: High priority user request (priority: 8)
  Task 3: Medium priority maintenance (priority: 3)
  Task 4: Critical security update (priority: 10)
  Task 5: Medium-high priority sync (priority: 5)

Processing tasks in priority order:
  1. Task 4: Critical security update (priority: 10)
  2. Task 2: High priority user request (priority: 8)
  3. Task 5: Medium-high priority sync (priority: 5)
  4. Task 3: Medium priority maintenance (priority: 3)
  5. Task 1: Low priority background job (priority: 1)

Example 3: Using MoosicBoxSender trait
---------------------------------------
Messages sent via MoosicBoxSender trait:
  - !Important message
  - Normal message
  - Another normal message
```

## Code Walkthrough

### Creating a Prioritized Channel

The example starts by creating an unbounded prioritized channel:

```rust
use moosicbox_channel_utils::futures_channel::unbounded;

let (tx, mut rx) = unbounded::<String>();
```

This creates a sender-receiver pair that supports priority-based message ordering.

### Configuring Priority Functions

Priority is configured using the `with_priority()` method on the sender:

```rust
let tx = tx.with_priority(|msg: &String| {
    if msg.contains("CRITICAL") { 100 }
    else if msg.contains("HIGH") { 50 }
    else if msg.contains("MEDIUM") { 25 }
    else { 1 }
});
```

The priority function receives a reference to each message and returns a `usize` value. Higher numbers indicate higher priority.

### Sending Messages

Messages are sent using the `MoosicBoxSender` trait's `send()` method:

```rust
use moosicbox_channel_utils::MoosicBoxSender;

tx.send("CRITICAL system failure".to_string())?;
tx.send("Normal message".to_string())?;
```

When a priority function is configured, messages are buffered internally and sorted by priority. Note that the first message sent is delivered immediately to prime the channel, and subsequent messages are buffered and sorted before being flushed in priority order as the receiver processes items.

### Receiving Messages

Messages are received using the `Stream` trait from `futures`:

```rust
use futures::StreamExt;

while let Some(message) = rx.next().await {
    println!("Received: {}", message);
}
```

The receiver automatically flushes the sender's priority buffer, ensuring messages are delivered in priority order.

### Custom Types with Priority Fields

For custom types, you can use struct fields to determine priority:

```rust
#[derive(Debug, Clone)]
struct Task {
    id: u32,
    priority: u8,
    description: String,
}

let (tx, mut rx) = unbounded::<Task>();
let tx = tx.with_priority(|task: &Task| task.priority as usize);
```

### Using the Trait for Generic Functions

The `PrioritizedSender` implements the `MoosicBoxSender` trait, allowing for generic sender functions:

```rust
fn send_via_trait<S, E>(sender: &S, msg: String) -> Result<(), E>
where
    S: MoosicBoxSender<String, E>,
{
    sender.send(msg)
}
```

## Key Concepts

### Priority Buffering

The prioritized sender uses a smart buffering strategy. The first message is sent immediately to "prime" the channel, ensuring the receiver can start processing. After that, subsequent messages are buffered internally and sorted by priority. When the receiver polls for the next item (by calling `.next().await`), the buffer is flushed and the highest-priority message is sent. This continues until all buffered messages are processed in priority order.

### Priority Function Flexibility

The priority function can be based on any aspect of the message:

- **Content-based**: Analyzing message content (keywords, patterns)
- **Field-based**: Using specific struct fields
- **Computed**: Calculating priority based on multiple factors
- **Dynamic**: Changing priority based on system state

### Stream Integration

The `PrioritizedReceiver` implements the `Stream` trait from `futures`, making it compatible with all stream combinators and utilities. This allows for easy integration with existing async code.

### Thread Safety

The prioritized sender can be cloned and shared across threads safely, as it uses `Arc` and `RwLock` internally to manage the priority buffer.

## Testing the Example

1. **Run the example** and observe the output order
2. **Modify priority values** in the priority functions to see how it affects ordering
3. **Add more messages** with different priorities to test buffer behavior
4. **Change the priority function** to use different logic (e.g., message length, timestamp)
5. **Create your own custom types** with priority fields

## Troubleshooting

### Messages Not Arriving in Priority Order

- Ensure you're using `with_priority()` on the sender before sending messages
- Verify that your priority function returns appropriate values (higher = higher priority)
- Make sure you're awaiting the receiver properly to allow buffer flushing

### Compilation Errors

- Check that `moosicbox_channel_utils` is using the `futures-channel` feature (enabled by default)
- Ensure the `tokio` runtime is properly configured with the `macros` and `rt-multi-thread` features
- Verify that your custom types implement the required traits (`Send`, `Clone` if needed)

### Performance Considerations

- Priority buffering adds overhead compared to direct channel sends
- Large priority buffers may impact performance; consider bounded alternatives for high-throughput scenarios
- The internal buffer uses a `RwLock`, which may cause contention under high concurrent load

## Related Examples

This is currently the only example for `moosicbox_channel_utils`. As more examples are added, they will be listed here.
