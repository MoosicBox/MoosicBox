# MoosicBox Tunnel

Core types and data structures for MoosicBox's tunneling protocol. This package defines the message formats and stream handling for communication between tunnel clients and servers in the MoosicBox ecosystem.

## Features

- **Tunnel Protocol Types**: Request and response message types for tunnel communication
- **Stream Handling**: Asynchronous stream for receiving chunked tunnel responses
- **Multiple Encodings**: Support for binary and base64-encoded messages
- **Request Types**: HTTP, WebSocket, and abort request handling
- **Packet Ordering**: Automatic packet reordering for out-of-sequence responses

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_tunnel = "0.1.4"
```

## Usage

### Tunnel Request Types

```rust
use moosicbox_tunnel::{TunnelRequest, TunnelHttpRequest, TunnelWsRequest, TunnelAbortRequest, TunnelEncoding};
use switchy_http::models::Method;
use serde_json::json;

// HTTP request through tunnel
let http_request = TunnelRequest::Http(TunnelHttpRequest {
    request_id: 123,
    method: Method::Get,
    path: "/api/tracks".to_string(),
    query: json!({}),
    payload: None,
    headers: None,
    encoding: TunnelEncoding::Binary,
    profile: Some("default".to_string()),
});

// WebSocket request through tunnel
let ws_request = TunnelRequest::Ws(TunnelWsRequest {
    conn_id: 456,
    request_id: 124,
    body: json!({"action": "subscribe"}),
    connection_id: None,
    profile: Some("default".to_string()),
});

// Abort a request
let abort_request = TunnelRequest::Abort(TunnelAbortRequest {
    request_id: 123,
});
```

### Tunnel Response Handling

```rust
use moosicbox_tunnel::TunnelResponse;
use bytes::Bytes;

// Parse binary tunnel response
let bytes = Bytes::from(vec![/* binary data */]);
let response: TunnelResponse = bytes.try_into()?;

println!("Request ID: {}", response.request_id);
println!("Packet ID: {}", response.packet_id);
println!("Last packet: {}", response.last);
if let Some(status) = response.status {
    println!("HTTP Status: {}", status);
}
```

### Base64 Encoded Responses

```rust
use moosicbox_tunnel::TunnelResponse;

#[cfg(feature = "base64")]
{
    let base64_str = "TUNNEL_RESPONSE:123|1|1|200{...}";
    let response: TunnelResponse = base64_str.try_into()?;
    println!("Decoded response: {:?}", response);
}
```

### Tunnel Stream

```rust
use moosicbox_tunnel::TunnelStream;
use switchy_async::sync::mpsc;
use switchy_async::util::CancellationToken;
use futures_util::StreamExt;

async fn handle_tunnel_stream() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::unbounded();
    let request_id = 123;
    let abort_token = CancellationToken::new();

    let on_end = |req_id: u64| async move {
        println!("Stream ended for request {}", req_id);
        Ok(())
    };

    let mut stream = TunnelStream::new(request_id, rx, abort_token, &on_end);

    // Consume stream
    while let Some(result) = stream.next().await {
        match result {
            Ok(bytes) => {
                println!("Received {} bytes", bytes.len());
            }
            Err(e) => {
                eprintln!("Stream error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}
```

## Core Types

### TunnelRequest

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum TunnelRequest {
    Http(TunnelHttpRequest),
    Ws(TunnelWsRequest),
    Abort(TunnelAbortRequest),
}
```

### TunnelResponse

```rust
pub struct TunnelResponse {
    pub request_id: u64,
    pub packet_id: u32,
    pub last: bool,
    pub bytes: Bytes,
    pub status: Option<u16>,
    pub headers: Option<BTreeMap<String, String>>,
}
```

### TunnelWsResponse

```rust
pub struct TunnelWsResponse {
    pub request_id: u64,
    pub body: Value,
    pub exclude_connection_ids: Option<Vec<u64>>,
    pub to_connection_ids: Option<Vec<u64>>,
}
```

### TunnelStream

A `Stream` implementation that:

- Receives `TunnelResponse` packets via an unbounded channel
- Automatically reorders out-of-sequence packets
- Tracks metrics (packet count, byte count, timing)
- Supports cancellation via `CancellationToken`
- Calls an `on_end` callback when the stream completes

### TunnelEncoding

```rust
pub enum TunnelEncoding {
    Binary,
    #[cfg(feature = "base64")]
    Base64,
}
```

## Features

### `base64` (default)

Enables base64 encoding/decoding support for tunnel responses.

```toml
[dependencies]
moosicbox_tunnel = { version = "0.1.4", default-features = false }
```

## Error Types

### TryFromBytesError

Errors when converting bytes to `TunnelResponse`:

- `TryFromSlice`: Invalid byte slice length
- `Serde`: JSON deserialization error

### Base64DecodeError

Errors when decoding base64-encoded tunnel responses:

- `InvalidContent`: Malformed base64 string
- `Decode`: Base64 decoding error

### TunnelStreamError

Errors during stream processing:

- `Aborted`: Stream was cancelled
- `EndOfStream`: Stream ended unexpectedly

## Dependencies

Core dependencies (from `Cargo.toml`):

- `bytes` - Efficient byte buffer handling
- `futures-util` - Stream trait implementation
- `serde` / `serde_json` - Serialization support
- `switchy_async` - Cancellation token support
- `switchy_http` - HTTP method types
- `tokio` - Async runtime support

## See Also

- [`moosicbox_tunnel_server`](../tunnel_server/README.md) - Server-side tunnel implementation
- [`moosicbox_tunnel_sender`](../tunnel_sender/README.md) - Client-side tunnel sender
- [`moosicbox_ws`](../ws/README.md) - WebSocket communication layer
