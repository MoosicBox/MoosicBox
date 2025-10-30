# Basic Byte Broadcasting Example

This example demonstrates how to use `ByteWriter` and `ByteStream` to broadcast bytes to multiple concurrent readers.

## Summary

Shows how to write data once and have it automatically broadcast to multiple stream readers simultaneously using the `ByteWriter` and `ByteStream` primitives.

## What This Example Demonstrates

- Creating a `ByteWriter` for broadcasting data
- Creating multiple `ByteStream` instances from a single writer
- Writing data that gets broadcast to all streams simultaneously
- Properly closing the writer to signal stream completion
- Reading from multiple streams concurrently
- Tracking total bytes written

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with the `futures::Stream` trait
- Understanding of tokio runtime and tasks

## Running the Example

```bash
cargo run --manifest-path packages/stream_utils/examples/basic_broadcast/Cargo.toml
```

## Expected Output

```
=== Basic Byte Broadcasting Example ===

Created ByteWriter with ID: 1
Created 3 streams from the writer

Stream 1: Starting to read...
Stream 2: Starting to read...
Stream 3: Starting to read...

Writing data to ByteWriter...
Wrote: "Hello, "
Stream 1: Received 7 bytes
Stream 2: Received 7 bytes
Stream 3: Received 7 bytes
Wrote: "streaming "
Stream 1: Received 10 bytes
Stream 2: Received 10 bytes
Stream 3: Received 10 bytes
Wrote: "world!"
Stream 1: Received 6 bytes
Stream 2: Received 6 bytes
Stream 3: Received 6 bytes

Closing the writer...

Waiting for all streams to complete...

Stream 1: Received end signal
Stream 1: Complete message: "Hello, streaming world!"
Stream 2: Received end signal
Stream 2: Complete message: "Hello, streaming world!"
Stream 3: Received end signal
Stream 3: Complete message: "Hello, streaming world!"

=== Results ===
Stream 1 received: "Hello, streaming world!"
Stream 2 received: "Hello, streaming world!"
Stream 3 received: "Hello, streaming world!"

✓ All streams received the same data successfully!
✓ Total bytes written: 23
```

## Code Walkthrough

### 1. Creating the Writer

```rust
let mut writer = ByteWriter::default();
```

Creates a new `ByteWriter` with a unique ID. The writer implements `std::io::Write` and broadcasts all written data to connected streams.

### 2. Creating Multiple Streams

```rust
let stream1 = writer.stream();
let stream2 = writer.stream();
let stream3 = writer.stream();
```

Each call to `writer.stream()` creates a new `ByteStream` that will receive a copy of all data written to the writer.

### 3. Spawning Reader Tasks

```rust
let handle1 = tokio::spawn(async move {
    let mut stream = stream1;
    while let Some(result) = stream.next().await {
        match result {
            Ok(bytes) => { /* process bytes */ }
            Err(e) => { /* handle error */ }
        }
    }
});
```

Each stream is moved into its own tokio task to read concurrently. `ByteStream` yields `Result<Bytes, std::io::Error>` items.

### 4. Writing Data

```rust
writer.write_all(b"Hello, ")?;
writer.write_all(b"streaming ")?;
writer.write_all(b"world!")?;
```

Data written to the writer is immediately broadcast to all connected streams. Each stream receives its own copy.

### 5. Closing the Writer

```rust
writer.close();
```

Signals all streams that no more data will be sent by sending an empty `Bytes` signal. This causes streams to complete their iteration.

## Key Concepts

### Broadcasting Pattern

`ByteWriter` implements a single-producer, multiple-consumer pattern. Data is written once but distributed to all active streams:

- **Write once**: Data is written to the writer via `std::io::Write` methods
- **Broadcast automatically**: All connected streams receive the data
- **Independent consumption**: Each stream can be read at its own pace

### Stream Lifecycle

1. Create writer
2. Create streams from writer (can be done at any time)
3. Write data (streams receive it)
4. Close writer (streams receive end signal)
5. Streams complete when they receive empty bytes

### Error Handling

`ByteStream` yields `Result<Bytes, std::io::Error>`. While the current implementation rarely produces errors, the Result type allows for future error conditions.

### Memory Management

- Disconnected streams are automatically removed from the writer
- Data is cloned for each stream, so memory usage scales with the number of streams
- Use unbounded channels internally for maximum throughput

## Testing the Example

The example includes assertions to verify all streams receive identical data:

```rust
assert_eq!(result1, "Hello, streaming world!");
assert_eq!(result2, "Hello, streaming world!");
assert_eq!(result3, "Hello, streaming world!");
```

## Troubleshooting

### Streams Don't Receive Data

- Ensure `writer.close()` is called to signal completion
- Check that tasks are given time to start before writing begins
- Verify streams are being polled (e.g., in spawned tasks)

### Data Appears Out of Order

- Data order is preserved for each individual stream
- Timing of when streams process data may vary between streams
- Use proper synchronization if ordering across streams matters

## Related Examples

- `typed_broadcast` - Broadcasting typed values instead of raw bytes
- `stalled_monitor` - Adding timeout detection to streams
