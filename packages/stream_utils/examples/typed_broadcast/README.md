# Typed Value Broadcasting Example

This example demonstrates how to use `TypedWriter<T>` and `TypedStream<T>` to broadcast strongly-typed values to multiple concurrent readers.

## Summary

Shows how to broadcast custom types (not just bytes) to multiple stream readers, enabling type-safe event distribution and pub-sub patterns.

## What This Example Demonstrates

- Creating a `TypedWriter<T>` for any cloneable type T
- Broadcasting strings to multiple streams
- Broadcasting custom enum types (events)
- Multiple consumers processing the same typed data differently
- Implementing event-driven architectures with type safety
- Stream lifecycle management (completion when writer is dropped)

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with the `futures::Stream` trait
- Understanding of Rust generics and trait bounds
- Basic knowledge of tokio runtime and tasks

## Running the Example

```bash
cargo run --manifest-path packages/stream_utils/examples/typed_broadcast/Cargo.toml
```

## Expected Output

```
=== Typed Value Broadcasting Example ===

--- Example 1: String Broadcasting ---

Created TypedWriter<String>
Created 2 streams

Stream 1: Starting to read strings...
Stream 2: Starting to read strings...
Writing strings to the TypedWriter...
Stream 1: Received: "Hello from TypedWriter!"
Stream 2: Received: "Hello from TypedWriter!"
Stream 1: Received: "This is message 2"
Stream 2: Received: "This is message 2"
Stream 1: Received: "Final message"
Stream 2: Received: "Final message"
Dropping the writer to signal completion...

Stream 1: Completed with 3 messages
Stream 2: Completed with 3 messages
✓ Both streams received identical data!

--- Example 2: Custom Event Broadcasting ---

Created TypedWriter<Event>
Created 3 streams (logger, analytics, notifier)

[Logger] Starting event logging...
[Analytics] Starting event analysis...
[Notifier] Starting notification service...
Broadcasting events...

[Logger] Event #1: UserJoined { username: "Alice", user_id: 1 }
[Notifier] 📢 Welcome Alice!
[Logger] Event #2: UserJoined { username: "Bob", user_id: 2 }
[Notifier] 📢 Welcome Bob!
[Logger] Event #3: MessageSent { from: "Alice", message: "Hello everyone!" }
[Logger] Event #4: MessageSent { from: "Bob", message: "Hi Alice!" }
[Logger] Event #5: UserJoined { username: "Charlie", user_id: 3 }
[Notifier] 📢 Welcome Charlie!
[Logger] Event #6: MessageSent { from: "Charlie", message: "Hey folks!" }
[Logger] Event #7: UserLeft { username: "Bob" }
[Notifier] 👋 Goodbye Bob!

Dropping writer to complete streams...

[Logger] Finished logging 7 events
[Analytics] Summary:
  - User joins: 3
  - Messages: 3
  - User leaves: 1
[Notifier] Sent 4 notifications

=== Results ===
Total events processed: 7
Events by type: 3 joins, 3 messages, 1 leaves
Notifications sent: 4

✓ All event consumers processed events correctly!
```

## Code Walkthrough

### 1. Defining Custom Types

```rust
#[derive(Debug, Clone, PartialEq)]
enum Event {
    UserJoined { username: String, user_id: u64 },
    MessageSent { from: String, message: String },
    UserLeft { username: String },
}
```

`TypedWriter<T>` works with any type that implements `Clone`. Custom types enable type-safe event distribution.

### 2. Creating a Typed Writer

```rust
let writer = TypedWriter::<Event>::default();
```

Creates a writer for the specific type. Unlike `ByteWriter` which only handles bytes, `TypedWriter` preserves type information.

### 3. Creating Typed Streams

```rust
let logger_stream = writer.stream();
let analytics_stream = writer.stream();
let notifier_stream = writer.stream();
```

Each stream receives the full typed value (not raw bytes), enabling different consumers to process events in domain-specific ways.

### 4. Writing Typed Values

```rust
writer.write(Event::UserJoined {
    username: "Alice".to_string(),
    user_id: 1,
});
```

Values are cloned and broadcast to all streams. The last stream receives the original value (optimization to avoid unnecessary clone).

### 5. Stream Completion

```rust
drop(writer);
```

When the writer is dropped, all streams naturally complete their iteration. No explicit close method is needed for `TypedWriter`.

## Key Concepts

### Type Safety

`TypedWriter<T>` and `TypedStream<T>` provide compile-time type safety:

- **Compile-time guarantees**: Writer and streams must agree on type T
- **No serialization overhead**: Values are passed directly, not encoded
- **Pattern matching**: Consumers can use Rust's pattern matching on typed values

### Pub-Sub Pattern

The example demonstrates a publish-subscribe architecture:

- **Publisher**: The `TypedWriter` publishes events
- **Subscribers**: Multiple `TypedStream` instances subscribe to events
- **Independent processing**: Each subscriber processes events differently
    - Logger: Records all events
    - Analytics: Counts event types
    - Notifier: Sends notifications for specific events

### Clone Behavior

`TypedWriter<T>` requires `T: Clone`:

- Values are cloned for each stream except the last
- The last stream receives the original value (performance optimization)
- All streams receive semantically identical values

### Stream vs ByteStream

Compared to `ByteStream`:

- **TypedStream**: Yields items of type `T` directly
- **ByteStream**: Yields `Result<Bytes, std::io::Error>`
- TypedStream doesn't wrap items in Result (errors handled at channel level)

## Testing the Example

The example includes comprehensive assertions:

```rust
assert_eq!(event_count, 7);
assert_eq!(joins, 3);
assert_eq!(messages, 3);
assert_eq!(leaves, 1);
```

Modify the event sequence to experiment with different patterns.

## Troubleshooting

### Compilation Error: T Does Not Implement Clone

Ensure your type implements `Clone`:

```rust
#[derive(Clone)]
struct MyType { /* fields */ }
```

### Streams Don't Complete

- Ensure the writer is dropped or goes out of scope
- `TypedWriter` doesn't have an explicit `close()` method - drop it instead
- Verify streams are being actively polled in tasks

### Values Received Out of Order

- Values are delivered in order to each individual stream
- Different streams may process at different rates
- Use synchronization primitives if cross-stream ordering matters

## Related Examples

- `basic_broadcast` - Broadcasting raw bytes with `ByteWriter`
- `stalled_monitor` - Adding timeout detection to typed streams
