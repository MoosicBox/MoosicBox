#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! TCP Echo Server Example
//!
//! This example demonstrates basic TCP server/client usage with `switchy_tcp`.
//! It creates a simple echo server that accepts connections and echoes back
//! any data it receives.

use std::env;

use switchy_tcp::{GenericTcpListener, TokioTcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Runs the echo server on the specified address
async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Bind the TCP listener to the address
    println!("Starting echo server on {addr}");
    let listener = TokioTcpListener::bind(addr).await?;
    println!("Echo server listening on {addr}");

    loop {
        // Accept incoming connections
        let (mut stream, client_addr) = listener.accept().await?;
        println!("New connection from: {client_addr}");

        // Spawn a task to handle each connection concurrently
        tokio::spawn(async move {
            let mut buffer = [0u8; 1024];

            loop {
                // Read data from the client
                match stream.read(&mut buffer).await {
                    Ok(0) => {
                        // Connection closed
                        println!("Client {client_addr} disconnected");
                        break;
                    }
                    Ok(n) => {
                        // Echo the data back to the client
                        let data = &buffer[..n];
                        println!(
                            "Received {} bytes from {client_addr}: {:?}",
                            n,
                            String::from_utf8_lossy(data)
                        );

                        if let Err(e) = stream.write_all(data).await {
                            eprintln!("Error writing to {client_addr}: {e}");
                            break;
                        }

                        println!("Echoed {n} bytes back to {client_addr}");
                    }
                    Err(e) => {
                        eprintln!("Error reading from {client_addr}: {e}");
                        break;
                    }
                }
            }
        });
    }
}

/// Runs the echo client that connects to the server and sends test messages
async fn run_client(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use switchy_tcp::TokioTcpStream;

    // Connect to the server
    println!("Connecting to server at {addr}");
    let mut stream = TokioTcpStream::connect(addr).await?;

    // Display connection information
    println!("Connected to server at {}", stream.peer_addr()?);
    println!("Local address: {}", stream.local_addr()?);

    // Send test messages
    let messages = ["Hello, server!", "How are you?", "Goodbye!"];

    for msg in messages {
        // Send the message
        println!("Sending: {msg}");
        stream.write_all(msg.as_bytes()).await?;

        // Read the echo response
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!("Received: {response}");

        // Wait a bit between messages
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    println!("Client finished");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} [server|client] [address]", args[0]);
        eprintln!("Example: {} server 127.0.0.1:8080", args[0]);
        eprintln!("Example: {} client 127.0.0.1:8080", args[0]);
        std::process::exit(1);
    }

    let mode = &args[1];
    let addr = args.get(2).map_or("127.0.0.1:8080", String::as_str);

    match mode.as_str() {
        "server" => run_server(addr).await?,
        "client" => run_client(addr).await?,
        _ => {
            eprintln!("Invalid mode: {mode}");
            eprintln!("Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}
