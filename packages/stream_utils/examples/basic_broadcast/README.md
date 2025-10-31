# Basic Broadcast Example

This example demonstrates how to use `ByteWriter` and `ByteStream` to broadcast bytes to multiple concurrent readers.

## Summary

Shows the fundamental pattern of creating a `ByteWriter`, spawning multiple `ByteStream` readers, writing data that gets broadcast to all readers, and properly closing the writer.

## What This Example Demonstrates

- Creating a `ByteWriter` instance with a unique ID
- Creating multiple `ByteStream` readers from a single writer
- Broadcasting data to all connected streams simultaneously
- Each stream receives an independent copy of the data
- Proper stream lifecycle management with `writer.close()`
- Tracking total bytes written with `writer.bytes_written()`
- Handling the `Result<Bytes, std::io::Error>` items from `ByteStream`
- Concurrent reading from multiple streams using tokio tasks

## Prerequisites

- Basic understanding of Rust async programming
- Familiarity with the `futures::Stream` trait
- Understanding of tokio runtime and tasks

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/stream_utils/examples/basic_broadcast/Cargo.toml
```

Or from the example directory:

```bash
cd packages/stream_utils/examples/basic_broadcast
cargo run
```

## Expected Output

```
=== Basic Broadcast Example ===

Created ByteWriter with ID: 1
Created 3 ByteStream readers

[Stream 1] Starting to read...
[Stream 2] Starting to read...
[Stream 3] Starting to read...
[Writer] Writing first message...
[Stream 1] Received chunk 1: 13 bytes - 'Hello, World!'
[Stream 2] Received chunk 1: 13 bytes
[Stream 3] Received chunk 1: 13 bytes
[Writer] Writing second message...
[Stream 1] Received chunk 2: 33 bytes - 'Broadcast message to all streams!'
[Stream 2] Received chunk 2: 33 bytes
[Stream 3] Received chunk 2: 33 bytes
[Writer] Writing third message...
[Stream 1] Received chunk 3: 29 bytes - 'Final message before closing.'
[Stream 2] Received chunk 3: 29 bytes
[Stream 3] Received chunk 3: 29 bytes
[Writer] Total bytes written: 75

[Writer] Closing writer...

[Stream 1] Received close signal
[Stream 1] Finished: 3 chunks, 75 total bytes

[Stream 2] Received close signal
[Stream 2] Finished: 3 chunks, 75 total bytes

[Stream 3] Received close signal
[Stream 3] Finished: 3 chunks, 75 total bytes

=== Example Complete ===

Key takeaways:
- ByteWriter broadcasts data to multiple ByteStream readers
- Each stream receives an independent copy of the data
- writer.close() sends an empty bytes signal to indicate completion
- ByteStream yields Result<Bytes, std::io::Error> items
```

## Code Walkthrough

### Creating the Writer and Streams

```rust
let mut writer = ByteWriter::default();
let mut stream1 = writer.stream();
let mut stream2 = writer.stream();
let mut stream3 = writer.stream();
```

Each call to `writer.stream()` creates a new `ByteStream` that will receive copies of all data written to the writer. The writer automatically manages the internal channels to each stream.

### Broadcasting Data

```rust
writer.write_all(b"Hello, World!")?;
```

The `ByteWriter` implements `std::io::Write`, so you can use familiar methods like `write_all()`. When you write data, it's automatically broadcast to all connected streams.

### Reading from Streams

```rust
while let Some(result) = stream1.next().await {
    match result {
        Ok(bytes) => {
            if bytes.is_empty() {
                // Close signal received
                break;
            }
            // Process the bytes
        }
        Err(e) => {
            // Handle error
        }
    }
}
```

Each `ByteStream` implements `futures::Stream` and yields `Result<Bytes, std::io::Error>`. An empty `Bytes` indicates the writer has been closed.

### Closing the Writer

```rust
writer.close();
```

Calling `close()` sends an empty bytes signal to all connected streams, allowing them to know that no more data will be written. This is important for proper cleanup and graceful shutdown.

## Key Concepts

### Broadcasting Pattern

The `ByteWriter`/`ByteStream` pair implements a one-to-many broadcasting pattern. One writer can send data to multiple readers simultaneously, with each reader getting its own independent copy of the data.

### Channel Management

Internally, the writer maintains unbounded channels to each stream. If a stream is disconnected (dropped), the writer automatically removes that channel on the next write operation.

### Lifecycle Management

- **Creation**: Create a writer, then create streams from it
- **Writing**: Write data using standard `std::io::Write` methods
- **Reading**: Each stream independently reads data asynchronously
- **Closing**: Call `writer.close()` to signal completion
- **Cleanup**: Streams automatically detect the close signal and complete

### Independent Readers

Each stream operates independently. One slow reader doesn't block others because each has its own unbounded channel. However, this means memory usage grows if a reader falls behind significantly.

## Testing the Example

Run the example multiple times and observe:

1. **Consistent behavior**: All three streams receive the same data in the same order
2. **Concurrent operation**: The streams read concurrently, though output order may vary
3. **Proper cleanup**: All streams detect the close signal and terminate cleanly
4. **Byte tracking**: The writer correctly reports the total number of bytes written

Try modifying the example:

- Add more streams
- Write different sizes or types of data
- Add delays between writes
- Drop a stream early and see automatic cleanup

## Troubleshooting

### Streams don't receive data

- Ensure you're calling `.await` on `stream.next()`
- Make sure the writer hasn't been dropped before writing
- Verify the streams were created before writing data

### Program hangs

- Make sure you're calling `writer.close()` to signal completion
- Ensure all async tasks are being properly awaited
- Check that streams are actually consuming data (calling `next().await`)

### Memory usage grows

- This is expected with unbounded channels if readers are slow
- Consider implementing backpressure or using bounded channels for production code
- Ensure streams are actively consuming data and not just accumulating

## Related Examples

- `typed_stream` - Demonstrates broadcasting typed data instead of raw bytes
- `stalled_monitoring` - Shows how to add timeout detection to streams
