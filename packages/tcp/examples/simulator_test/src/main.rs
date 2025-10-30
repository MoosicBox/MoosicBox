#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! TCP Simulator Testing Example
//!
//! This example demonstrates how to use the in-memory TCP simulator for testing
//! network code without actual network I/O. This is useful for:
//! - Deterministic testing
//! - Avoiding port conflicts
//! - Testing network code without network access
//! - Fast, reliable unit tests

use switchy_tcp::{GenericTcpListener, SimulatorTcpListener, SimulatorTcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Example of a simple protocol handler that can be tested
async fn handle_client(
    mut stream: SimulatorTcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = [0u8; 1024];

    // Read command from client
    let n = stream.read(&mut buffer).await?;
    let command = String::from_utf8_lossy(&buffer[..n]);

    println!("Server received command: {command}");

    // Process command and send response
    let response = match command.trim() {
        "PING" => "PONG",
        "HELLO" => "WORLD",
        "STATUS" => "OK",
        cmd => {
            println!("Unknown command: {cmd}");
            "ERROR: Unknown command"
        }
    };

    println!("Server sending response: {response}");
    stream.write_all(response.as_bytes()).await?;

    Ok(())
}

/// Test function demonstrating basic simulator usage
async fn test_basic_communication() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Test 1: Basic Communication ===");

    let addr = "127.0.0.1:8080";

    // Create a listener using the simulator
    let listener = SimulatorTcpListener::bind(addr).await?;
    println!("Simulator listener bound to {addr}");

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (stream, client_addr) = listener.accept().await.unwrap();
        println!("Server accepted connection from {client_addr}");
        handle_client(stream).await.unwrap();
    });

    // Create client connection
    let mut client = SimulatorTcpStream::connect(addr).await?;
    println!("Client connected to {addr}");

    // Send PING command
    client.write_all(b"PING").await?;
    println!("Client sent: PING");

    // Read response
    let mut buffer = [0u8; 1024];
    let n = client.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Client received: {response}");

    assert_eq!(response, "PONG", "Expected PONG response");

    // Wait for server task to complete
    server_task.await?;

    println!("✓ Test passed: Basic communication works");
    Ok(())
}

/// Test demonstrating multiple concurrent connections
async fn test_multiple_connections() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Test 2: Multiple Concurrent Connections ===");

    let addr = "127.0.0.1:8081";
    let listener = SimulatorTcpListener::bind(addr).await?;
    println!("Listener bound to {addr}");

    // Spawn server to handle multiple connections
    let server_task = tokio::spawn(async move {
        for i in 1..=3 {
            let (stream, client_addr) = listener.accept().await.unwrap();
            println!("Server accepted connection #{i} from {client_addr}");

            tokio::spawn(async move {
                handle_client(stream).await.unwrap();
            });
        }
    });

    // Create multiple client connections
    let commands = ["PING", "HELLO", "STATUS"];
    let mut client_tasks = vec![];

    for (i, command) in commands.iter().enumerate() {
        let command = (*command).to_string();
        let task = tokio::spawn(async move {
            let mut client = SimulatorTcpStream::connect("127.0.0.1:8081").await.unwrap();
            println!("Client #{} connected", i + 1);

            client.write_all(command.as_bytes()).await.unwrap();
            println!("Client #{} sent: {command}", i + 1);

            let mut buffer = [0u8; 1024];
            let n = client.read(&mut buffer).await.unwrap();
            let response = String::from_utf8_lossy(&buffer[..n]);
            println!("Client #{} received: {response}", i + 1);

            response.to_string()
        });

        client_tasks.push(task);
    }

    // Collect results
    let responses: Vec<String> = futures::future::join_all(client_tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Wait for server
    server_task.await?;

    // Verify responses
    assert_eq!(responses[0], "PONG");
    assert_eq!(responses[1], "WORLD");
    assert_eq!(responses[2], "OK");

    println!("✓ Test passed: Multiple concurrent connections work");
    Ok(())
}

/// Test demonstrating stream splitting with simulator
async fn test_split_stream() -> Result<(), Box<dyn std::error::Error>> {
    use switchy_tcp::GenericTcpStream;

    println!("\n=== Test 3: Split Stream Communication ===");

    let addr = "127.0.0.1:8082";
    let listener = SimulatorTcpListener::bind(addr).await?;
    println!("Listener bound to {addr}");

    // Server with split stream
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut read_half, mut write_half) = stream.into_split();

        // Read in one task
        let reader = tokio::spawn(async move {
            let mut buffer = [0u8; 1024];
            let n = read_half.read(&mut buffer).await.unwrap();
            String::from_utf8_lossy(&buffer[..n]).to_string()
        });

        // Write in another task
        let writer = tokio::spawn(async move {
            write_half.write_all(b"Server response").await.unwrap();
        });

        let (received, _) = tokio::join!(reader, writer);
        received.unwrap()
    });

    // Client with split stream
    let client = SimulatorTcpStream::connect(addr).await?;
    let (mut read_half, mut write_half) = client.into_split();

    // Write from client
    write_half.write_all(b"Client message").await?;

    // Read response
    let mut buffer = [0u8; 1024];
    let n = read_half.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);

    let server_received = server_task.await?;

    assert_eq!(server_received, "Client message");
    assert_eq!(response, "Server response");

    println!("✓ Test passed: Split stream communication works");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Switchy TCP Simulator Testing Example");
    println!("======================================");
    println!("This example demonstrates testing TCP code with the in-memory simulator");

    // Run all tests
    test_basic_communication().await?;
    test_multiple_connections().await?;
    test_split_stream().await?;

    println!("\n✓ All tests passed successfully!");
    println!("\nKey benefits of the simulator:");
    println!("  - No actual network I/O (fast and deterministic)");
    println!("  - No port conflicts");
    println!("  - Works without network access");
    println!("  - Perfect for unit testing");

    Ok(())
}
