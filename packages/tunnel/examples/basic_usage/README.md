# Basic Tunnel Usage Example

A comprehensive example demonstrating the core functionality of the `moosicbox_tunnel` package, including creating tunnel requests, parsing responses, and handling chunked data with `TunnelStream`.

## Summary

This example shows how to use the MoosicBox tunnel protocol for creating and handling HTTP and WebSocket requests over a persistent tunnel connection. It demonstrates request creation, response parsing from binary format, and asynchronous stream handling for chunked responses.

## What This Example Demonstrates

- Creating different types of tunnel requests:
    - HTTP GET requests with query parameters and headers
    - HTTP POST requests with JSON payloads
    - WebSocket requests for real-time communication
    - Abort requests to cancel in-progress operations
- Parsing tunnel responses from binary byte format
- Using `TunnelStream` to consume chunked responses asynchronously
- Understanding the packet structure and ordering system
- Working with both binary and base64 encoding formats (when feature enabled)

## Prerequisites

- Basic understanding of Rust async programming with Tokio
- Familiarity with HTTP requests and WebSocket communication
- Understanding of streaming and chunked data transfer

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/tunnel/examples/basic_usage/Cargo.toml
```

To run with only binary encoding (disable base64 feature):

```bash
cargo run --manifest-path packages/tunnel/examples/basic_usage/Cargo.toml --no-default-features
```

## Expected Output

The example will output structured information showing:

```
🔌 MoosicBox Tunnel - Basic Usage Example

This example demonstrates the core features of moosicbox_tunnel:
  • Creating tunnel requests (HTTP, WebSocket, Abort)
  • Parsing tunnel responses from binary format
  • Using TunnelStream for chunked response handling

=== Tunnel Request Types ===

HTTP GET Request:
{
  "type": "HTTP",
  "request_id": 1,
  "method": "GET",
  "path": "/api/tracks",
  "query": {"artist": "Test Artist"},
  ...
}

HTTP POST Request:
{
  "type": "HTTP",
  "request_id": 2,
  "method": "POST",
  ...
}

WebSocket Request:
{
  "type": "WS",
  "conn_id": 100,
  ...
}

Abort Request:
{
  "type": "ABORT",
  "request_id": 1
}

=== Binary Response Parsing ===

Binary data length: 127 bytes
Successfully parsed tunnel response:
  Request ID: 1
  Packet ID: 1
  Last packet: true
  Body length: 30 bytes
  HTTP Status: 200
  Headers:
    content-type: application/json
    server: moosicbox-tunnel
  Body: Response packet 1 for request 1

=== Tunnel Stream ===

Simulating a multi-packet response stream...
  Sent packet 1/3 (last: false)
  Sent packet 2/3 (last: false)
  Sent packet 3/3 (last: true)

Consuming stream packets:
  Packet 1: 30 bytes
    Content: Response packet 1 for request 5
  Packet 2: 30 bytes
    Content: Response packet 2 for request 5
  Packet 3: 30 bytes
    Content: Response packet 3 for request 5

Stream completed for request 5
Total: 3 packets, 90 bytes

✅ Example completed successfully!
```

## Code Walkthrough

### Creating Tunnel Requests

The example demonstrates creating various tunnel request types using the `TunnelRequest` enum:

```rust
// HTTP GET request
let http_request = TunnelRequest::Http(TunnelHttpRequest {
    request_id: 1,
    method: Method::Get,
    path: "/api/tracks".to_string(),
    query: json!({"artist": "Test Artist"}),
    payload: None,
    headers: Some(json!({"Authorization": "Bearer token123"})),
    encoding: TunnelEncoding::Binary,
    profile: Some("default".to_string()),
});

// WebSocket request
let ws_request = TunnelRequest::Ws(TunnelWsRequest {
    conn_id: 100,
    request_id: 3,
    body: json!({"action": "subscribe", "channel": "playback"}),
    connection_id: Some(json!(42)),
    profile: Some("default".to_string()),
});

// Abort request
let abort_request = TunnelRequest::Abort(TunnelAbortRequest { request_id: 1 });
```

Each request type is serializable to JSON for transmission over the tunnel connection.

### Parsing Binary Responses

Tunnel responses are transmitted as binary packets with a specific structure:

- 8 bytes: Request ID (u64, big-endian)
- 4 bytes: Packet ID (u32, big-endian)
- 1 byte: Last packet flag (0 or 1)
- If packet ID = 1:
    - 2 bytes: HTTP status code (u16)
    - 4 bytes: Headers JSON length (u32)
    - N bytes: Headers JSON data
- Remaining bytes: Response body

The `TunnelResponse::try_from()` method handles this parsing:

```rust
let binary_data = create_mock_binary_response(1, 1, true);
let response: TunnelResponse = binary_data.try_into()?;

println!("Request ID: {}", response.request_id);
println!("Packet ID: {}", response.packet_id);
println!("Last packet: {}", response.last);
if let Some(status) = response.status {
    println!("HTTP Status: {}", status);
}
```

### Using TunnelStream

`TunnelStream` implements the `Stream` trait for handling multi-packet responses:

```rust
// Create channel for responses
let (tx, rx) = unbounded_channel();

// Create cancellation token
let abort_token = CancellationToken::new();

// Define completion callback
let on_end = |req_id: u64| async move {
    println!("Stream completed for request {}", req_id);
    Ok(())
};

// Create stream
let mut stream = TunnelStream::new(request_id, rx, abort_token, &on_end);

// Consume packets
while let Some(result) = stream.next().await {
    match result {
        Ok(bytes) => println!("Received {} bytes", bytes.len()),
        Err(e) => eprintln!("Error: {:?}", e),
    }
}
```

The stream automatically handles:

- Out-of-order packet delivery (packets are reordered by packet_id)
- Performance metrics tracking (time to first byte, total time)
- Graceful completion when the last packet is received
- Cancellation support via `CancellationToken`

## Key Concepts

### Request Identification

Every tunnel request has a unique `request_id` that allows the protocol to:

- Match responses to their originating requests
- Handle multiple concurrent requests over the same connection
- Support request cancellation via abort messages

### Packet Ordering

Responses are sent as ordered packets (starting from packet_id = 1). The first packet includes:

- HTTP status code
- Response headers
- Initial body data

Subsequent packets contain only body data. `TunnelStream` ensures packets are delivered to the application in order, even if they arrive out of sequence.

### Encoding Formats

The protocol supports two encoding formats:

- **Binary**: Raw bytes for efficient transmission
- **Base64**: Text-safe encoding for environments requiring string-based protocols

The encoding is specified in the request and determines how the response is formatted.

### Stream Management

`TunnelStream` provides:

- **Asynchronous iteration**: Compatible with `futures::Stream`
- **Automatic buffering**: Queues out-of-order packets until ready
- **Metrics tracking**: Monitors byte count, packet count, and timing
- **Cancellation support**: Allows aborting streams via `CancellationToken`
- **Completion callbacks**: Notifies when stream finishes via `on_end`

## Testing the Example

To verify the example works correctly:

1. **Run the example**: Execute the command shown in "Running the Example"
2. **Verify output**: Check that all sections display correctly formatted data
3. **Check packet handling**: Confirm the TunnelStream section shows 3 packets received in order
4. **Inspect parsing**: Verify binary response parsing shows correct request_id, packet_id, status, and headers

The example uses mock data to simulate the tunnel protocol without requiring a real tunnel connection, making it ideal for understanding the API.

## Troubleshooting

### Compilation Errors

**Problem**: Cannot find `switchy_async` or `switchy_http` modules

**Solution**: Ensure you're running from the workspace root with the correct manifest path:

```bash
cargo run --manifest-path packages/tunnel/examples/basic_usage/Cargo.toml
```

### Base64 Feature

**Problem**: Base64 functionality not available

**Solution**: The `base64` feature is enabled by default. If you've disabled default features, enable it explicitly:

```toml
moosicbox_tunnel = { workspace = true, features = ["base64"] }
```

### Stream Not Completing

If implementing your own stream consumer and the stream doesn't complete:

- Ensure you're dropping the sender (`tx`) after sending the last packet
- Verify the last packet has `last: true` set
- Check that `packet_id` values are sequential starting from 1

## Related Examples

- **Tunnel Server Implementation**: See `packages/tunnel_server` for server-side tunnel handling
- **Tunnel Sender Implementation**: See `packages/tunnel_sender` for client-side request sending
- **WebSocket Communication**: See `packages/ws` for the underlying WebSocket layer
- **HTTP Request Examples**: See `packages/http/examples/simple_get` for basic HTTP client usage
