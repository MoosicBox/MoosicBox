#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! TCP Stream Splitting Example
//!
//! This example demonstrates how to split a TCP stream into separate read and write
//! halves to enable concurrent reading and writing operations. This is useful for
//! full-duplex communication where you want to read and write simultaneously.

use std::time::Duration;

use switchy_tcp::{GenericTcpListener, GenericTcpStream, TokioTcpListener, TokioTcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Server that handles bidirectional communication with split streams
async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = "127.0.0.1:8080";
    println!("Starting server on {addr}");
    let listener = TokioTcpListener::bind(addr).await?;
    println!("Server listening on {addr}");

    // Accept one connection for this example
    let (stream, client_addr) = listener.accept().await?;
    println!("Client connected from: {client_addr}");

    // Split the stream into read and write halves
    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn a task to continuously read from the client
    let reader_handle = tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        let mut message_count = 0;

        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) => {
                    println!("Client disconnected");
                    break;
                }
                Ok(n) => {
                    message_count += 1;
                    let msg = String::from_utf8_lossy(&buffer[..n]);
                    println!("Received message #{message_count}: {msg}");
                }
                Err(e) => {
                    eprintln!("Read error: {e}");
                    break;
                }
            }
        }

        message_count
    });

    // Spawn a task to periodically write to the client
    let writer_handle = tokio::spawn(async move {
        let messages = [
            "Server status: Running",
            "Server status: Processing",
            "Server status: Ready",
            "Server status: Shutting down",
        ];

        for (i, msg) in messages.iter().enumerate() {
            println!("Sending message #{}: {msg}", i + 1);

            if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                eprintln!("Write error: {e}");
                break;
            }

            // Wait between messages
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        messages.len()
    });

    // Wait for both tasks to complete
    let (read_count, write_count) = tokio::join!(reader_handle, writer_handle);
    println!(
        "Server finished: read {} messages, wrote {} messages",
        read_count.unwrap_or(0),
        write_count.unwrap_or(0)
    );

    Ok(())
}

/// Client that demonstrates concurrent reading and writing
async fn run_client() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait a bit for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let addr = "127.0.0.1:8080";
    println!("Connecting to server at {addr}");
    let stream = TokioTcpStream::connect(addr).await?;

    println!("Connected to server at {}", stream.peer_addr()?);
    println!("Local address: {}", stream.local_addr()?);

    // Split the stream into read and write halves
    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn a task to continuously read from the server
    let reader_handle = tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        let mut message_count = 0;

        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) => {
                    println!("Server closed connection");
                    break;
                }
                Ok(n) => {
                    message_count += 1;
                    let msg = String::from_utf8_lossy(&buffer[..n]);
                    println!("Received from server #{message_count}: {msg}");
                }
                Err(e) => {
                    eprintln!("Read error: {e}");
                    break;
                }
            }
        }

        message_count
    });

    // Spawn a task to periodically write to the server
    let writer_handle = tokio::spawn(async move {
        let messages = [
            "Client: Hello!",
            "Client: Sending data",
            "Client: More data",
            "Client: Final message",
        ];

        for (i, msg) in messages.iter().enumerate() {
            println!("Sending to server #{}: {msg}", i + 1);

            if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                eprintln!("Write error: {e}");
                break;
            }

            // Wait between messages
            tokio::time::sleep(Duration::from_millis(800)).await;
        }

        messages.len()
    });

    // Wait for both tasks to complete
    let (read_count, write_count) = tokio::join!(reader_handle, writer_handle);
    println!(
        "Client finished: read {} messages, wrote {} messages",
        read_count.unwrap_or(0),
        write_count.unwrap_or(0)
    );

    // Give a moment for final messages to be received
    tokio::time::sleep(Duration::from_millis(500)).await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run both server and client concurrently
    println!("Starting bidirectional communication example");
    println!("This demonstrates concurrent reading and writing with split streams\n");

    let server_handle = tokio::spawn(run_server());
    let client_handle = tokio::spawn(run_client());

    // Wait for both to complete
    let (server_result, client_result) = tokio::join!(server_handle, client_handle);

    if let Err(e) = server_result {
        eprintln!("Server task error: {e}");
    }

    if let Err(e) = client_result {
        eprintln!("Client task error: {e}");
    }

    println!("\nExample completed successfully!");
    Ok(())
}
