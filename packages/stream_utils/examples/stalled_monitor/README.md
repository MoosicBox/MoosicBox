# Stalled Read Monitor Example

This example demonstrates how to use `StalledReadMonitor` to add timeout detection and rate limiting to streams.

## Summary

Shows how to wrap any stream with `StalledReadMonitor` to detect when data flow stalls and enforce timeout or throttling policies.

## What This Example Demonstrates

- Wrapping streams with stalled read monitoring
- Detecting stream timeouts when no data is received
- Throttling stream consumption to limit read rate
- Combining timeout and throttle policies
- Error handling for timeout conditions
- Real-world use cases for flow control

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with the `futures::Stream` trait
- Understanding of tokio runtime and time utilities
- Knowledge of timeout and rate limiting concepts

## Running the Example

```bash
cargo run --manifest-path packages/stream_utils/examples/stalled_monitor/Cargo.toml
```

## Expected Output

```
=== Stalled Read Monitor Example ===

--- Example 1: Timeout Detection ---

Creating a stream that will stall...
Created monitored stream with 2-second timeout

Writer: Sending first chunk...
Reader: Received chunk 1 (11 bytes)
Writer: Sending second chunk...
Reader: Received chunk 2 (12 bytes)
Writer: Waiting 3 seconds (longer than 2-second timeout)...
Reader: Monitor error - Stalled
Reader: Stream timed out after no data for 2 seconds!

✓ Reader detected timeout as expected

--- Example 2: Stream Throttling ---

Creating a throttled stream...
Created throttled stream (500ms between reads)

Writer: Sending data rapidly...

Writer: Sent chunk 1
Writer: Sent chunk 2
Writer: Sent chunk 3
Writer: Sent chunk 4
Writer: Sent chunk 5
Reader: Chunk 1 received after 0.5s
Reader: Chunk 2 received after 0.5s
Reader: Chunk 3 received after 0.5s
Reader: Chunk 4 received after 0.5s
Reader: Chunk 5 received after 0.5s
Reader: Received end signal

✓ Throttling enforced: 5 chunks read with 500ms delays

--- Example 3: Combined Timeout and Throttling ---

Creating stream with both timeout and throttling...
Created stream with 3s timeout and 200ms throttle

Writer: Sending chunks at different intervals...

Reader: Chunk 1 at 0.2s (7 bytes)
Reader: Chunk 2 at 0.5s (7 bytes)
Reader: Chunk 3 at 0.8s (7 bytes)
Reader: Chunk 4 at 1.0s (7 bytes)
Reader: Received end signal

✓ Successfully read 4 chunks with combined timeout and throttling
```

## Code Walkthrough

### 1. Creating a Monitored Stream

```rust
let stream = writer.stream();
let mut monitored_stream = stream
    .stalled_monitor()
    .with_timeout(Duration::from_secs(2));
```

The `stalled_monitor()` method is available on `ByteStream` when the `stalled-monitor` feature is enabled. It wraps the stream in a `StalledReadMonitor`.

### 2. Configuring Timeout

```rust
.with_timeout(Duration::from_secs(2))
```

Sets a timeout duration. If no data is received within this period, the monitor returns an `std::io::Error` with `ErrorKind::TimedOut`. The timeout resets each time data is received.

### 3. Configuring Throttling

```rust
.with_throttle(Duration::from_millis(500))
```

Enforces a minimum delay between reads. The stream will wait at least this duration before yielding the next item, effectively rate-limiting consumption.

### 4. Handling Monitor Results

```rust
while let Some(result) = monitored_stream.next().await {
    match result {
        Ok(bytes_result) => {
            // Handle the inner stream's result (ByteStream yields Result)
            match bytes_result {
                Ok(bytes) => { /* process bytes */ }
                Err(e) => { /* handle ByteStream error */ }
            }
        }
        Err(e) => {
            // Handle monitor errors (timeout, etc.)
            if e.kind() == std::io::ErrorKind::TimedOut {
                // Stream timed out
            }
        }
    }
}
```

`StalledReadMonitor` wraps stream items in `Result<T>`, so when monitoring a `ByteStream` (which yields `Result<Bytes, io::Error>`), you get `Result<Result<Bytes, io::Error>>`.

### 5. Combining Policies

```rust
let mut monitored_stream = stream
    .stalled_monitor()
    .with_timeout(Duration::from_secs(3))
    .with_throttle(Duration::from_millis(200));
```

Both timeout and throttling can be applied together. The monitor will enforce the throttle delay AND detect timeouts.

## Key Concepts

### Timeout Detection

The stalled monitor tracks time since the last successful read:

- **Timeout resets**: Each time data is received, the timeout timer resets
- **Stall detection**: If no data arrives within the timeout period, returns `TimedOut` error
- **Grace period**: The stream has the full timeout duration to produce each item

### Throttling Mechanism

Throttling limits how fast items can be consumed:

- **Minimum delay**: Enforces a minimum time between reads
- **Rate limiting**: Prevents overwhelming downstream consumers
- **Backpressure**: Naturally applies backpressure to the writer

### Error Types

`StalledReadMonitor` can return these errors:

- **`std::io::ErrorKind::TimedOut`**: Stream stalled (no data within timeout)
- Wraps the inner stream's items in `Result`, preserving their error types

### Use Cases

**Timeout Detection**:

- Detecting network stalls in streaming
- Preventing hung operations
- Implementing health checks
- Failing fast on unresponsive sources

**Throttling**:

- Rate limiting API consumption
- Preventing resource exhaustion
- Smoothing bursty traffic
- Implementing backpressure

## Testing the Example

The example includes three scenarios:

1. **Timeout scenario**: Writer intentionally delays longer than timeout
2. **Throttling scenario**: Writer sends rapidly, reader throttles consumption
3. **Combined scenario**: Both policies work together

Experiment by adjusting timeout and throttle durations to see different behaviors.

## Troubleshooting

### Timeout Triggers Immediately

- Ensure timeout duration is longer than expected data arrival time
- Check that the stream is being actively polled
- Verify data is actually being written to the stream

### Throttling Not Working

- Confirm the throttle duration is longer than natural read intervals
- Ensure `with_throttle()` is called on the monitor
- Check that timing measurements account for throttle delays

### Double Result Handling is Confusing

- Remember: `StalledReadMonitor` wraps items in `Result`
- For `ByteStream`, you get `Result<Result<Bytes, io::Error>>`
- Outer `Result`: Monitor errors (timeout, etc.)
- Inner `Result`: Stream's own errors

### Stream Completes Without Timeout

- Timeout only triggers when stream is pending (waiting for data)
- If stream completes normally, no timeout occurs
- Timeout is for detecting stalls, not enforcing maximum duration

## Related Examples

- `basic_broadcast` - Basic byte streaming without monitoring
- `typed_broadcast` - Can also be monitored (works with any stream)
- `remote_streaming` - Real-world use case for timeout detection
