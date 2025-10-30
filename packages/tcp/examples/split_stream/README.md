# Split Stream Example

Demonstrates TCP stream splitting to enable concurrent reading and writing operations for full-duplex communication.

## Summary

This example shows how to split a TCP stream into separate read and write halves, allowing simultaneous reading and writing operations in different tasks. This is essential for full-duplex protocols where both parties need to send and receive data independently.

## What This Example Demonstrates

- Splitting TCP streams with `into_split()` method
- Concurrent reading and writing using separate tasks
- Full-duplex bidirectional communication
- Using the `GenericTcpStream` trait
- Coordinating multiple async tasks with `tokio::join!`
- Graceful handling of connection closure from both sides

## Prerequisites

- Understanding of async Rust and Tokio
- Familiarity with TCP networking
- Basic knowledge of concurrent programming concepts
- Understanding of TCP stream read/write operations

## Running the Example

Simply run the example, which starts both server and client automatically:

```bash
cargo run --manifest-path packages/tcp/examples/split_stream/Cargo.toml
```

## Expected Output

```
Starting bidirectional communication example
This demonstrates concurrent reading and writing with split streams

Starting server on 127.0.0.1:8080
Server listening on 127.0.0.1:8080
Connecting to server at 127.0.0.1:8080
Client connected from: 127.0.0.1:XXXXX
Connected to server at 127.0.0.1:8080
Local address: 127.0.0.1:XXXXX
Sending to server #1: Client: Hello!
Sending message #1: Server status: Running
Received message #1: Client: Hello!
Received from server #1: Server status: Running
Sending to server #2: Client: Sending data
Received message #2: Client: Sending data
Sending message #2: Server status: Processing
Received from server #2: Server status: Processing
Sending message #3: Server status: Ready
Sending to server #3: Client: More data
Received from server #3: Server status: Ready
Received message #3: Client: More data
Sending to server #4: Client: Final message
Received message #4: Client: Final message
Sending message #4: Server status: Shutting down
Received from server #4: Server status: Shutting down
Client finished: read 4 messages, wrote 4 messages
Server finished: read 4 messages, wrote 4 messages

Example completed successfully!
```

## Code Walkthrough

### Splitting the Stream

Both server and client split their streams into read and write halves:

```rust
let (mut read_half, mut write_half) = stream.into_split();
```

This returns separate owned halves that can be moved into different tasks.

### Concurrent Reading Task

A task is spawned to handle reading independently:

```rust
let reader_handle = tokio::spawn(async move {
    let mut buffer = [0u8; 1024];
    loop {
        match read_half.read(&mut buffer).await {
            Ok(0) => break, // Connection closed
            Ok(n) => {
                // Process received data
            }
            Err(e) => break,
        }
    }
});
```

### Concurrent Writing Task

Another task handles writing independently:

```rust
let writer_handle = tokio::spawn(async move {
    for msg in messages {
        write_half.write_all(msg.as_bytes()).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
});
```

### Coordinating Tasks

Both tasks are coordinated using `tokio::join!`:

```rust
let (read_count, write_count) = tokio::join!(reader_handle, writer_handle);
```

## Key Concepts

### Stream Splitting

Stream splitting allows a single TCP connection to be used by multiple tasks concurrently. The `into_split()` method consumes the stream and returns owned read and write halves.

### Full-Duplex Communication

With split streams, both parties can send and receive data simultaneously without blocking each other. The reader task doesn't block the writer task and vice versa.

### Owned Halves

The split halves are owned, meaning they can be moved into separate tasks. This is different from borrowed splits (like `split()` in some libraries) which require the halves to stay in the same scope.

### Generic Traits

The example uses the `GenericTcpStream` trait which provides the `into_split()` method. This works with both `TokioTcpStream` and `SimulatorTcpStream`.

## Testing the Example

1. Run the example and observe the interleaved output
2. Notice how messages from both directions are being sent and received concurrently
3. Observe the timing - messages are sent at different intervals but don't block each other
4. Check the final counts to verify all messages were transmitted successfully

## Troubleshooting

**Messages appear out of order:**

- This is expected with concurrent operations
- The example uses timing to demonstrate concurrency
- Output order may vary between runs

**One task finishes before the other:**

- This is normal - tasks run at different speeds
- The `tokio::join!` waits for both to complete
- Check for errors in task results

**Connection closes prematurely:**

- Verify both tasks are handling errors properly
- Check that writes complete before the connection is dropped
- Ensure proper timing for the example to complete

## Related Examples

- `echo_server` - Basic TCP server/client without stream splitting
- `simulator_test` - Testing TCP communication with the simulator
