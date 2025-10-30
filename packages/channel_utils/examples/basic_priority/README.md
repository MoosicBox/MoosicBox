# Basic Priority Example

This example demonstrates the core functionality of `moosicbox_channel_utils` - prioritized message passing through async channels using the `PrioritizedSender` and `PrioritizedReceiver` types.

## Summary

Learn how to create prioritized channels that automatically reorder messages based on custom priority functions, ensuring high-priority messages are processed before lower-priority ones.

## What This Example Demonstrates

- Creating unbounded prioritized channels with `unbounded()`
- Configuring priority functions using `with_priority()`
- Sending messages with the `MoosicBoxSender` trait
- Receiving messages in priority order using `StreamExt`
- Priority ordering with string content analysis
- Priority ordering with structured data types
- Regular FIFO behavior when no priority function is configured

## Prerequisites

- Basic understanding of Rust async programming
- Familiarity with futures and streams
- Knowledge of tokio runtime
- Understanding of channel-based message passing

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/channel_utils/examples/basic_priority/Cargo.toml
```

## Expected Output

The example runs three scenarios demonstrating different priority configurations:

```
=== MoosicBox Channel Utils - Basic Priority Example ===

Example 1: String messages with priority keywords

Sending messages:
  Sent: LOW: Regular system update
  Sent: MEDIUM: User notification pending
  Sent: LOW: Background task completed
  Sent: CRITICAL: Security alert detected!
  Sent: HIGH: Database connection lost
  Sent: MEDIUM: Cache invalidation needed
  Sent: CRITICAL: Memory threshold exceeded!

Receiving messages in priority order:
  Received: CRITICAL: Security alert detected!
  Received: CRITICAL: Memory threshold exceeded!
  Received: HIGH: Database connection lost
  Received: MEDIUM: User notification pending
  Received: MEDIUM: Cache invalidation needed
  Received: LOW: Regular system update
  Received: LOW: Background task completed

---

Example 2: Task queue with explicit priorities

Queuing tasks:
  Queued: Task 1 - 'Backup database' (priority: 3)
  Queued: Task 2 - 'Process payments' (priority: 10)
  Queued: Task 3 - 'Send emails' (priority: 1)
  Queued: Task 4 - 'Handle user request' (priority: 8)
  Queued: Task 5 - 'Update cache' (priority: 2)
  Queued: Task 6 - 'Emergency shutdown' (priority: 10)

Processing tasks in priority order:
  Processing: Task 2 - 'Process payments' (priority: 10)
  Processing: Task 6 - 'Emergency shutdown' (priority: 10)
  Processing: Task 4 - 'Handle user request' (priority: 8)
  Processing: Task 1 - 'Backup database' (priority: 3)
  Processing: Task 5 - 'Update cache' (priority: 2)
  Processing: Task 3 - 'Send emails' (priority: 1)

---

Example 3: Regular channel without priority (FIFO)

Sending messages:
  Sent: First message
  Sent: Second message
  Sent: Third message
  Sent: Fourth message

Receiving messages in FIFO order:
  Received: First message
  Received: Second message
  Received: Third message
  Received: Fourth message
```

## Code Walkthrough

### Creating a Prioritized Channel

The example starts by creating an unbounded prioritized channel:

```rust
use moosicbox_channel_utils::futures_channel::unbounded;

let (tx, mut rx) = unbounded::<String>();
```

This creates a sender-receiver pair similar to `futures::channel::mpsc::unbounded()`, but with built-in priority support.

### Configuring Priority Function

Priority is configured by calling `with_priority()` on the sender:

```rust
let tx = tx.with_priority(|msg: &String| {
    if msg.contains("CRITICAL") {
        100  // Highest priority
    } else if msg.contains("HIGH") {
        50
    } else if msg.contains("MEDIUM") {
        25
    } else {
        1    // Lowest priority
    }
});
```

The priority function receives a reference to each message and returns a `usize` value. Higher values mean higher priority. Messages with the same priority are processed in FIFO order.

### Sending Messages

Messages are sent using the `MoosicBoxSender` trait's `send()` method:

```rust
use moosicbox_channel_utils::MoosicBoxSender;

tx.send("CRITICAL: Security alert detected!".to_string())?;
tx.send("LOW: Background task completed".to_string())?;
```

The sender internally buffers messages and maintains priority ordering.

### Receiving Messages in Priority Order

The receiver implements the `Stream` trait from `futures_core`:

```rust
use futures::StreamExt;

while let Some(message) = rx.next().await {
    println!("Received: {message}");
}
```

As the receiver polls for messages, the sender's internal buffer is automatically flushed in priority order, ensuring high-priority messages are received first.

### Priority with Structured Data

The example also demonstrates priority ordering with custom types:

```rust
#[derive(Debug, Clone)]
struct Task {
    id: u32,
    name: String,
    priority: u8,
}

let (tx, mut rx) = unbounded::<Task>();
let tx = tx.with_priority(|task: &Task| task.priority as usize);
```

This pattern is useful for task queues, job schedulers, or any scenario where messages have explicit priority fields.

### FIFO Behavior Without Priority

When no priority function is configured, the channel behaves as a standard FIFO channel:

```rust
let (tx, mut rx) = unbounded::<String>();
// No .with_priority() call - messages are processed in FIFO order
```

## Key Concepts

### Priority Buffering

The `PrioritizedSender` maintains an internal buffer of messages sorted by priority. When you call `send()`:

1. If no priority function is configured, the message is sent immediately (FIFO)
2. If a priority function is configured and the receiver is not ready, the message is buffered
3. Messages in the buffer are sorted by priority value (highest first)
4. When the receiver polls, buffered messages are flushed in priority order

### Automatic Flush Mechanism

The `PrioritizedReceiver` automatically flushes the sender's buffer when polling for messages. This design ensures:

- No manual buffer management required
- Priority ordering happens transparently
- Backpressure is naturally handled through the internal buffer

### Priority Function Guidelines

When designing priority functions:

- Use higher numeric values for higher priority
- Keep priority calculations fast (they run on every send)
- Consider using priority levels (e.g., 1, 10, 100) rather than fine-grained values
- Messages with equal priority are processed in FIFO order
- The priority function has access to message content but not mutable state

### Use Cases

Priority channels are ideal for:

- **Alert Systems**: Ensure critical alerts are processed before informational ones
- **Task Queues**: Execute high-priority tasks before background jobs
- **Request Handling**: Prioritize user-facing requests over internal operations
- **Resource Management**: Handle resource-critical operations first
- **Event Processing**: Process urgent events before routine updates

## Testing the Example

To verify the example works correctly:

1. **Check Priority Ordering**: In Example 1, verify that CRITICAL messages appear first in the output, followed by HIGH, MEDIUM, and LOW messages

2. **Verify Task Prioritization**: In Example 2, confirm that tasks with priority 10 are processed first, followed by priority 8, 3, 2, and 1

3. **Confirm FIFO Behavior**: In Example 3, ensure messages are received in the exact order they were sent

4. **Modify Priority Functions**: Try changing the priority values in the code and observe how the output changes

5. **Add More Messages**: Add additional messages with different priorities to see how they're ordered

## Troubleshooting

### Messages Not in Priority Order

**Issue**: Messages are received in FIFO order instead of priority order

**Solution**: Ensure you called `.with_priority()` on the sender before sending messages:

```rust
let tx = tx.with_priority(|msg| /* priority logic */);
```

### Compilation Errors with StreamExt

**Issue**: `rx.next()` method not found

**Solution**: Import the `StreamExt` trait:

```rust
use futures::StreamExt;
```

### Runtime Errors

**Issue**: "no reactor is running" or similar tokio errors

**Solution**: Ensure you're using the tokio runtime with the correct features:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Your code here
}
```

And in Cargo.toml:

```toml
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Related Examples

This is currently the only example for `moosicbox_channel_utils`. For more information about channel usage patterns, see:

- Package documentation: `packages/channel_utils/README.md`
- API documentation: Run `cargo doc --open` in the package directory
