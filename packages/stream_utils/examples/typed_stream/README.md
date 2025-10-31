# Typed Stream Example

This example demonstrates how to use `TypedWriter` and `TypedStream` to broadcast strongly-typed values to multiple concurrent readers.

## Summary

Shows how to create a `TypedWriter` for custom types, broadcast typed events to multiple specialized consumers, and leverage Rust's type system for compile-time safety.

## What This Example Demonstrates

- Creating a `TypedWriter` for custom enum types
- Broadcasting strongly-typed events to multiple `TypedStream` readers
- Each stream independently processing events for different purposes
- Type safety ensuring only correct types can be sent/received
- Pattern matching on received events for specialized handling
- Multiple concurrent consumers with different processing logic
- Automatic stream closure when the writer is dropped
- Clone-based broadcasting for efficient multi-reader distribution

## Prerequisites

- Understanding of Rust enums and pattern matching
- Basic knowledge of async programming with tokio
- Familiarity with the `futures::Stream` trait
- Understanding of type parameters and generics

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/stream_utils/examples/typed_stream/Cargo.toml
```

Or from the example directory:

```bash
cd packages/stream_utils/examples/typed_stream
cargo run
```

## Expected Output

```
=== Typed Stream Example ===

Created TypedWriter for Event type

[Logger] Starting to log events...
[Metrics] Starting to collect metrics...
[Alert Monitor] Starting to monitor system alerts...

[Writer] Broadcasting events...

[Logger] Event #1: UserLogin { username: "alice", timestamp: 1234567890 }
[Logger] Event #2: SystemAlert { level: Info, message: "System started successfully" }
[Alert Monitor] INFO: System started successfully
[Logger] Event #3: DataUpdate { key: "temperature", value: 72 }
[Logger] Event #4: UserLogin { username: "bob", timestamp: 1234567900 }
[Logger] Event #5: SystemAlert { level: Warning, message: "High memory usage detected" }
[Alert Monitor] WARNING: High memory usage detected
[Logger] Event #6: DataUpdate { key: "humidity", value: 65 }
[Logger] Event #7: UserLogin { username: "charlie", timestamp: 1234567910 }
[Logger] Event #8: SystemAlert { level: Error, message: "Database connection lost" }
[Alert Monitor] ERROR: Database connection lost
[Logger] Event #9: DataUpdate { key: "pressure", value: 1013 }

[Writer] Dropping writer to close all streams...

[Logger] Finished logging 9 events

[Metrics] Final counts:
  - User logins: 3
  - Data updates: 3
  - System alerts: 3

[Alert Monitor] Detected 1 errors

=== Example Complete ===

Key takeaways:
- TypedWriter broadcasts strongly-typed values to multiple readers
- Each stream can process events differently based on their purpose
- Type safety prevents sending wrong data types to streams
- Values must implement Clone for broadcasting
- Dropping the writer closes all connected streams
```

## Code Walkthrough

### Defining Custom Types

```rust
#[derive(Clone, Debug)]
enum Event {
    UserLogin { username: String, timestamp: u64 },
    DataUpdate { key: String, value: i32 },
    SystemAlert { level: AlertLevel, message: String },
}
```

The type must implement `Clone` because the writer broadcasts by cloning values to each stream (except the last one). The `Debug` trait is useful for logging but not required.

### Creating the Writer and Streams

```rust
let writer = TypedWriter::<Event>::default();
let mut event_logger = writer.stream();
let mut metrics_collector = writer.stream();
let mut alert_monitor = writer.stream();
```

Each call to `writer.stream()` creates a new `TypedStream<Event>` that will receive typed events. The type parameter ensures compile-time type safety.

### Broadcasting Typed Values

```rust
writer.write(Event::UserLogin {
    username: "alice".to_string(),
    timestamp: 1234567890,
});
```

The `write()` method takes ownership of the value and broadcasts it to all connected streams. The value is cloned for each stream except the last, which receives the original value for efficiency.

### Processing Events with Pattern Matching

```rust
while let Some(event) = stream.next().await {
    match event {
        Event::UserLogin { username, timestamp } => {
            // Handle login event
        }
        Event::DataUpdate { key, value } => {
            // Handle data update
        }
        Event::SystemAlert { level, message } => {
            // Handle alert
        }
    }
}
```

Each stream receives the full `Event` type and can use pattern matching to handle different event variants. This allows specialized processing logic for each consumer.

### Specialized Stream Consumers

The example demonstrates three different consumer patterns:

1. **Event Logger**: Logs all events for audit trail
2. **Metrics Collector**: Aggregates event counts by type
3. **Alert Monitor**: Filters and processes only system alerts

This shows how the same event stream can serve multiple purposes simultaneously.

## Key Concepts

### Type Safety

Unlike `ByteWriter`, which broadcasts raw bytes, `TypedWriter` ensures type safety at compile time. You cannot accidentally send the wrong type to a stream, preventing entire classes of runtime errors.

### Clone Semantics

The writer clones values for broadcasting:

- For N streams, the value is cloned N-1 times
- The last stream receives the original value (no clone)
- This is an optimization to avoid unnecessary cloning

Your type must implement `Clone` for this to work.

### Independent Processing

Each stream independently processes events:

- Different streams can consume at different rates
- One slow stream doesn't block others (unbounded channels)
- Each stream can implement different filtering or processing logic
- Streams can be dropped independently without affecting others

### Stream Lifecycle

- **Creation**: Create a writer, then create typed streams from it
- **Writing**: Call `writer.write(value)` to broadcast
- **Reading**: Each stream yields items of type `T` (not `Result<T>`)
- **Closing**: Drop the writer or all senders disconnect
- **Cleanup**: Streams complete when the writer is dropped

### Comparison to ByteWriter

| Feature           | ByteWriter         | TypedWriter      |
| ----------------- | ------------------ | ---------------- |
| Item type         | `Bytes`            | Generic `T`      |
| Stream yields     | `Result<Bytes, E>` | `T`              |
| Type safety       | Runtime (bytes)    | Compile-time     |
| Clone requirement | No                 | Yes (`T: Clone`) |
| Use case          | Raw byte streams   | Structured data  |

## Testing the Example

Run the example and observe:

1. **Concurrent processing**: All three consumers process events simultaneously
2. **Type safety**: Try changing event types to see compile-time errors
3. **Independent consumption**: Each consumer processes events differently
4. **Graceful shutdown**: All streams complete when writer is dropped

Try modifying the example:

- Add new event types to the enum
- Create additional specialized consumers
- Add delays to simulate slow consumers
- Implement filtering logic in different streams
- Use different types like `String`, `i32`, or custom structs

## Troubleshooting

### "the trait bound `Event: Clone` is not satisfied"

Your custom type must implement `Clone`. Add `#[derive(Clone)]` to your type definition or implement `Clone` manually.

### Streams don't receive events

- Ensure streams are created before writing events
- Check that tasks are spawned and awaited
- Make sure you're calling `.await` on `stream.next()`
- Verify the writer hasn't been dropped prematurely

### Memory usage grows

With unbounded channels, slow consumers can cause memory growth:

- Ensure all streams actively consume events
- Consider implementing backpressure for production use
- Monitor channel sizes in production

### Writer is dropped too early

```rust
// Bad: writer dropped before events are consumed
{
    let writer = TypedWriter::default();
    let stream = writer.stream();
    writer.write(value);
} // writer dropped here, stream closes immediately

// Good: writer lives until streams finish
let writer = TypedWriter::default();
let stream = writer.stream();
spawn(async move { /* consume stream */ });
writer.write(value);
drop(writer); // explicit drop after writing is done
```

## Related Examples

- `basic_broadcast` - Demonstrates the byte-based equivalent (`ByteWriter`/`ByteStream`)
- `stalled_monitoring` - Shows how to add timeout detection to streams
