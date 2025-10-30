#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Basic Tunnel Usage Example
//!
//! This example demonstrates the core functionality of the `moosicbox_tunnel` package:
//! - Creating different types of tunnel requests (HTTP, WebSocket, Abort)
//! - Parsing tunnel responses from binary and base64 formats
//! - Using `TunnelStream` to handle chunked responses asynchronously

use std::collections::BTreeMap;

use bytes::Bytes;
use futures_util::StreamExt;
use moosicbox_tunnel::{
    TunnelAbortRequest, TunnelEncoding, TunnelHttpRequest, TunnelRequest, TunnelResponse,
    TunnelStream, TunnelWsRequest,
};
use serde_json::json;
use switchy_async::util::CancellationToken;
use switchy_http::models::Method;
use tokio::sync::mpsc::unbounded_channel;

/// Demonstrates creating various tunnel request types
fn demonstrate_tunnel_requests() {
    println!("=== Tunnel Request Types ===\n");

    // Create an HTTP GET request through the tunnel
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

    println!("HTTP GET Request:");
    println!("{}", serde_json::to_string_pretty(&http_request).unwrap());
    println!();

    // Create an HTTP POST request with payload
    let http_post_request = TunnelRequest::Http(TunnelHttpRequest {
        request_id: 2,
        method: Method::Post,
        path: "/api/playlists".to_string(),
        query: json!({}),
        payload: Some(json!({"name": "My Playlist", "tracks": [1, 2, 3]})),
        headers: Some(json!({"Content-Type": "application/json"})),
        encoding: TunnelEncoding::Binary,
        profile: Some("default".to_string()),
    });

    println!("HTTP POST Request:");
    println!(
        "{}",
        serde_json::to_string_pretty(&http_post_request).unwrap()
    );
    println!();

    // Create a WebSocket request through the tunnel
    let ws_request = TunnelRequest::Ws(TunnelWsRequest {
        conn_id: 100,
        request_id: 3,
        body: json!({"action": "subscribe", "channel": "playback"}),
        connection_id: Some(json!(42)),
        profile: Some("default".to_string()),
    });

    println!("WebSocket Request:");
    println!("{}", serde_json::to_string_pretty(&ws_request).unwrap());
    println!();

    // Create an abort request to cancel an in-progress request
    let abort_request = TunnelRequest::Abort(TunnelAbortRequest { request_id: 1 });

    println!("Abort Request:");
    println!("{}", serde_json::to_string_pretty(&abort_request).unwrap());
    println!();
}

/// Creates a mock binary tunnel response for demonstration
fn create_mock_binary_response(request_id: u64, packet_id: u32, last: bool) -> Bytes {
    let mut data = Vec::new();

    // Add request_id (8 bytes)
    data.extend_from_slice(&request_id.to_be_bytes());

    // Add packet_id (4 bytes)
    data.extend_from_slice(&packet_id.to_be_bytes());

    // Add last flag (1 byte)
    data.push(u8::from(last));

    // If this is the first packet, add status and headers
    if packet_id == 1 {
        // Add status code (2 bytes)
        let status: u16 = 200;
        data.extend_from_slice(&status.to_be_bytes());

        // Add headers
        let headers = BTreeMap::from([
            ("content-type".to_string(), "application/json".to_string()),
            ("server".to_string(), "moosicbox-tunnel".to_string()),
        ]);
        let headers_json = serde_json::to_vec(&headers).unwrap();
        let headers_len = u32::try_from(headers_json.len()).unwrap();

        // Add headers length (4 bytes)
        data.extend_from_slice(&headers_len.to_be_bytes());

        // Add headers data
        data.extend_from_slice(&headers_json);
    }

    // Add response body
    let body = format!("Response packet {packet_id} for request {request_id}");
    data.extend_from_slice(body.as_bytes());

    Bytes::from(data)
}

/// Demonstrates parsing tunnel responses from binary format
fn demonstrate_binary_response_parsing() {
    println!("=== Binary Response Parsing ===\n");

    // Create a mock single-packet response
    let binary_data = create_mock_binary_response(1, 1, true);
    println!("Binary data length: {} bytes", binary_data.len());

    // Parse the binary data into a TunnelResponse
    match TunnelResponse::try_from(binary_data) {
        Ok(response) => {
            println!("Successfully parsed tunnel response:");
            println!("  Request ID: {}", response.request_id);
            println!("  Packet ID: {}", response.packet_id);
            println!("  Last packet: {}", response.last);
            println!("  Body length: {} bytes", response.bytes.len());

            if let Some(status) = response.status {
                println!("  HTTP Status: {status}");
            }

            if let Some(headers) = &response.headers {
                println!("  Headers:");
                for (key, value) in headers {
                    println!("    {key}: {value}");
                }
            }

            // Print the body content
            if let Ok(body_str) = std::str::from_utf8(&response.bytes) {
                println!("  Body: {body_str}");
            }
        }
        Err(e) => {
            eprintln!("Error parsing tunnel response: {e}");
        }
    }

    println!();
}

/// Demonstrates using `TunnelStream` to handle chunked responses
#[allow(clippy::future_not_send)]
async fn demonstrate_tunnel_stream() {
    println!("=== Tunnel Stream ===\n");

    // Create a channel for sending tunnel responses
    let (tx, rx) = unbounded_channel();
    let request_id = 5;

    // Create mock multi-packet response data
    println!("Simulating a multi-packet response stream...");

    // Send 3 packets through the channel
    for packet_id in 1..=3 {
        let is_last = packet_id == 3;
        let response_bytes = create_mock_binary_response(request_id, packet_id, is_last);
        let response = TunnelResponse::try_from(response_bytes).unwrap();
        tx.send(response).unwrap();
        println!("  Sent packet {packet_id}/3 (last: {is_last})");
    }

    // Drop the sender to close the channel after all packets are sent
    drop(tx);

    println!();

    // Create a cancellation token for aborting the stream if needed
    let abort_token = CancellationToken::new();

    // Create callback for when stream ends
    let on_end = |req_id: u64| async move {
        println!("Stream completed for request {req_id}");
        Ok::<(), Box<dyn std::error::Error>>(())
    };

    // Create the tunnel stream
    let mut stream = TunnelStream::new(request_id, rx, abort_token, &on_end);

    println!("Consuming stream packets:");

    let mut total_bytes = 0;
    let mut packet_count = 0;

    // Consume the stream
    while let Some(result) = stream.next().await {
        match result {
            Ok(bytes) => {
                packet_count += 1;
                total_bytes += bytes.len();
                println!("  Packet {packet_count}: {} bytes", bytes.len());

                // Print the packet content
                if let Ok(content) = std::str::from_utf8(&bytes) {
                    println!("    Content: {content}");
                }
            }
            Err(e) => {
                eprintln!("  Stream error: {e:?}");
                break;
            }
        }
    }

    println!();
    println!("Total: {packet_count} packets, {total_bytes} bytes");
    println!();
}

/// Demonstrates base64 response parsing (if the feature is enabled)
#[cfg(feature = "base64")]
fn demonstrate_base64_response_parsing() {
    println!("=== Base64 Response Parsing ===\n");

    // Note: This is a simplified example showing the format
    // In practice, the base64 encoding includes the full binary data
    println!(
        "Base64 encoding format: TUNNEL_RESPONSE:{{request_id}}|{{packet_id}}|{{last}}|{{status}}{{headers}}{{base64_body}}"
    );
    println!("Example: TUNNEL_RESPONSE:123|1|1|200{{...}}SGVsbG8gV29ybGQ=");
    println!();
    println!("(Full base64 decoding example omitted for brevity)");
    println!();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔌 MoosicBox Tunnel - Basic Usage Example\n");
    println!("This example demonstrates the core features of moosicbox_tunnel:");
    println!("  • Creating tunnel requests (HTTP, WebSocket, Abort)");
    println!("  • Parsing tunnel responses from binary format");
    println!("  • Using TunnelStream for chunked response handling");
    println!();

    // Demonstrate creating various tunnel request types
    demonstrate_tunnel_requests();

    // Demonstrate parsing binary tunnel responses
    demonstrate_binary_response_parsing();

    // Demonstrate using TunnelStream for chunked responses
    demonstrate_tunnel_stream().await;

    // Demonstrate base64 response parsing if feature is enabled
    #[cfg(feature = "base64")]
    demonstrate_base64_response_parsing();

    println!("✅ Example completed successfully!");
    println!();
    println!("Key takeaways:");
    println!("  • TunnelRequest is a tagged enum supporting HTTP, WebSocket, and Abort requests");
    println!("  • TunnelResponse can be parsed from binary bytes using try_into()");
    println!("  • TunnelStream implements Stream for handling chunked responses");
    println!(
        "  • The protocol supports packet ordering and reordering for out-of-sequence delivery"
    );
    println!("  • Base64 encoding is available for text-safe transmission (when enabled)");

    Ok(())
}
