# Stalled Monitoring Example

This example demonstrates how to use `StalledReadMonitor` to add timeout detection and throttling to streams, preventing hangs and controlling data consumption rate.

## Summary

Shows how to wrap streams with `StalledReadMonitor` to detect stalls with timeouts, throttle consumption rate, and combine both for comprehensive flow control.

## What This Example Demonstrates

- Wrapping streams with `stalled_monitor()` for timeout detection
- Configuring timeout duration with `with_timeout()`
- Detecting when streams stall and handling timeout errors
- Throttling stream consumption with `with_throttle()`
- Combining timeout and throttling for comprehensive flow control
- How monitored streams yield `Result<T>` to report timeout errors
- Timeout resets on successful data receipt
- Independent timeout and throttle interval management

## Prerequisites

- Understanding of async streams and futures
- Basic knowledge of tokio and async/await
- Familiarity with `ByteWriter` and `ByteStream` (see `basic_broadcast` example)
- Understanding of timeout and throttling concepts

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/stream_utils/examples/stalled_monitoring/Cargo.toml
```

Or from the example directory:

```bash
cd packages/stream_utils/examples/stalled_monitoring
cargo run
```

## Expected Output

```
=== Stalled Monitoring Example ===

--- Example 1: Timeout Detection ---

Created ByteStream with 2-second timeout
Writing initial data...

✓ Received: 12 bytes - 'Initial data'

Waiting 3 seconds before next write (will exceed timeout)...

Attempting to read...
✗ Stream timed out as expected: Custom { kind: TimedOut, error: Stalled }
   (No data received within 2-second timeout)

--- Example 2: Throttling ---

Created ByteStream with 500ms throttle
Writing multiple chunks rapidly...

[Writer] Wrote message 1
[Writer] Wrote message 2
[Writer] Wrote message 3
[Writer] Wrote message 4
[Writer] Wrote message 5

[Writer] All messages written, closing stream
[   100ms] Chunk 1: 9 bytes - 'Message 1'
[   600ms] Chunk 2: 9 bytes - 'Message 2'
[  1100ms] Chunk 3: 9 bytes - 'Message 3'
[  1600ms] Chunk 4: 9 bytes - 'Message 4'
[  2100ms] Chunk 5: 9 bytes - 'Message 5'
Received close signal

Received 5 chunks in 2.1s
Average time per chunk: ~420ms

Notice: Messages are consumed at ~500ms intervals despite rapid writing

--- Example 3: Combined Timeout and Throttling ---

Created ByteStream with 3-second timeout AND 300ms throttle
Writing data at irregular intervals...

[Writer] Wrote message 1
[Writer] Wrote message 2
[Writer] Wrote message 3
[Writer] Wrote message 4

[Writer] Waiting 4 seconds (will exceed timeout)...
[   100ms] ✓ Received: 'Message 1 (immediate)'
[   400ms] ✓ Received: 'Message 2 (after 400ms)'
[   900ms] ✓ Received: 'Message 3 (after 500ms)'
[  1400ms] ✓ Received: 'Message 4 (after 400ms)'
[  4400ms] ✗ Timed out: Custom { kind: TimedOut, error: Stalled }

Successfully received 4 messages before completion/timeout
[Writer] Wrote message 5 (but reader already timed out)

Notice: Throttling slowed consumption, but timeout still triggered

=== Example Complete ===

Key takeaways:
- StalledReadMonitor wraps streams to add timeout/throttling
- Timeouts prevent indefinite hangs when data stops flowing
- Throttling controls the rate of data consumption
- Both can be combined for comprehensive flow control
- Monitored streams yield Result<T> to report timeout errors
```

## Code Walkthrough

### Creating a Monitored Stream with Timeout

```rust
let stream = writer.stream();
let monitored = stream
    .stalled_monitor()
    .with_timeout(Duration::from_secs(2));
```

The `stalled_monitor()` method wraps the stream, and `with_timeout()` configures the timeout duration. If no data is received within 2 seconds, the stream returns a `TimedOut` error.

### Handling Timeout Errors

```rust
match monitored.next().await {
    Some(Ok(bytes_result)) => {
        // Successfully received data
        let bytes = bytes_result?;
        // Process bytes
    }
    Some(Err(e)) => {
        // Timeout occurred
        eprintln!("Stream timed out: {e}");
    }
    None => {
        // Stream ended normally
    }
}
```

Monitored streams yield `Result<T>` instead of just `T`. The `Err` variant indicates a timeout occurred.

### Adding Throttling

```rust
let monitored = stream
    .stalled_monitor()
    .with_throttle(Duration::from_millis(500));
```

Throttling limits how fast data is consumed. Even if data is available immediately, the stream waits at least 500ms between items.

### Combining Timeout and Throttling

```rust
let monitored = stream
    .stalled_monitor()
    .with_timeout(Duration::from_secs(3))
    .with_throttle(Duration::from_millis(300));
```

Both policies work together:

- Throttling slows consumption to at least 300ms per item
- Timeout triggers if no data arrives within 3 seconds
- Timeout is reset each time data is successfully received

## Key Concepts

### Timeout Behavior

**When timeout triggers:**

- No data received within the configured duration
- Returns `Err(std::io::Error)` with kind `TimedOut`
- Stream can be considered stalled/dead

**When timeout resets:**

- Each time data is successfully received
- Timer starts counting from zero again
- Allows for irregular but continuous data flow

**Use cases:**

- Detecting broken connections
- Preventing indefinite hangs
- Enforcing SLA requirements
- Failing fast when data source is unresponsive

### Throttling Behavior

**How it works:**

- Enforces minimum delay between items
- Even if data is buffered and available, waits the full throttle duration
- Independent of data arrival rate

**Use cases:**

- Rate limiting to prevent overwhelming downstream systems
- Controlling resource consumption
- Enforcing fair usage policies
- Simulating slow consumers for testing

### Combined Behavior

When both are configured:

1. Throttle delays consumption by its interval
2. Timeout counts time since last successful read
3. Both timers operate independently
4. Throttle can slow consumption enough to trigger timeout
5. Useful for enforcing both minimum rate and maximum delay

### Type Wrapping

```
ByteStream → Stream<Item = Result<Bytes, io::Error>>
                    ↓
StalledReadMonitor → Stream<Item = Result<Result<Bytes, io::Error>, io::Error>>
                                           ↑                          ↑
                                     original error          timeout error
```

The monitor wraps the original stream type:

- Inner `Result`: Original stream's error (e.g., I/O errors)
- Outer `Result`: Monitor's error (timeout)

### Works with Any Stream

`StalledReadMonitor` is generic and works with any stream type:

```rust
// Works with ByteStream
let monitored_bytes = byte_stream.stalled_monitor();

// Works with TypedStream
let monitored_events = typed_stream.stalled_monitor();

// Works with any futures::Stream
let monitored_custom = custom_stream.stalled_monitor();
```

## Testing the Example

Run the example and observe:

1. **Example 1**: Stream successfully times out after 3 seconds of inactivity
2. **Example 2**: Throttling enforces ~500ms intervals despite rapid writes
3. **Example 3**: Combined policies work together correctly

Try modifying the example:

- Adjust timeout and throttle durations
- Write data at different intervals
- Add more complex error handling
- Test with `TypedStream` instead of `ByteStream`
- Add logging to observe timing behavior

## Troubleshooting

### Timeout triggers too early

```rust
// Timeout is too short for your use case
.with_timeout(Duration::from_secs(1))  // Too short!

// Increase timeout to match expected delays
.with_timeout(Duration::from_secs(30))  // Better for slow sources
```

### Timeout never triggers

- Ensure you're actually calling `.await` on `stream.next()`
- Check that the writer isn't continuously sending data
- Verify the timeout duration is configured correctly
- Make sure the stream isn't being dropped prematurely

### Throttling too aggressive

```rust
// Throttle too long, making system seem slow
.with_throttle(Duration::from_secs(5))  // Too slow!

// Reduce throttle for faster consumption
.with_throttle(Duration::from_millis(100))  // Better
```

### Double Result handling confusion

Remember monitored streams yield `Result<Result<T, E1>, E2>`:

```rust
// Handle both error types
match monitored.next().await {
    Some(Ok(inner_result)) => {
        // Got data, but check inner result
        match inner_result {
            Ok(value) => { /* process value */ }
            Err(e) => { /* handle original stream error */ }
        }
    }
    Some(Err(e)) => {
        // Timeout error from monitor
    }
    None => { /* stream ended */ }
}
```

### Timeout resets unexpectedly

The timeout resets on each successful read. If you need a total elapsed time limit regardless of activity, you'll need to track that separately.

## Production Considerations

### Choosing Timeout Values

- **Fast networks**: 5-10 seconds
- **Slow/unreliable networks**: 30-60 seconds
- **Local processes**: 1-5 seconds
- **Interactive applications**: 10-30 seconds

### Choosing Throttle Values

- **Rate limiting**: Based on API limits or fair usage
- **Resource control**: Based on system capacity
- **Testing**: Slow enough to observe, fast enough to complete
- **Production**: Balance throughput with resource usage

### Error Handling

Always handle both error types appropriately:

- Timeout errors might be recoverable (retry logic)
- Original stream errors might be fatal
- Log errors with context for debugging
- Consider metrics/monitoring for timeout frequency

### Memory Considerations

Unbounded channels + throttling = potential memory growth:

- Slow consumers accumulate buffered data
- Monitor memory usage in production
- Consider bounded channels for production use
- Implement backpressure if needed

## Related Examples

- `basic_broadcast` - Demonstrates `ByteWriter`/`ByteStream` without monitoring
- `typed_stream` - Shows typed streams that can also be monitored
