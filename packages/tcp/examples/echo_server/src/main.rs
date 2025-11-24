#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Simple TCP echo server example using `switchy_tcp`.
//!
//! This example demonstrates:
//! - Binding a TCP listener to an address
//! - Accepting incoming connections
//! - Reading and writing data asynchronously
//! - Handling multiple concurrent connections
//! - Graceful shutdown on SIGINT

use std::error::Error;

use switchy_tcp::{GenericTcpListener, TokioTcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Bind the TCP listener to localhost on port 8080
    let listener = TokioTcpListener::bind("127.0.0.1:8080").await?;
    println!("Echo server listening on 127.0.0.1:8080");
    println!("Press Ctrl+C to stop the server");

    // Set up graceful shutdown on SIGINT (Ctrl+C)
    let mut shutdown = tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for SIGINT");
        println!("\nReceived SIGINT, shutting down...");
    });

    // Accept connections in a loop
    loop {
        tokio::select! {
            // Accept new connection
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        println!("New connection from: {addr}");

                        // Spawn a new task to handle this connection
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream).await {
                                eprintln!("Connection error from {addr}: {e}");
                            } else {
                                println!("Connection closed: {addr}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Failed to accept connection: {e}");
                    }
                }
            }
            // Shutdown signal received
            _ = &mut shutdown => {
                break;
            }
        }
    }

    Ok(())
}

/// Handles a single TCP connection by echoing back all received data.
///
/// # Errors
///
/// Returns an error if reading from or writing to the stream fails.
async fn handle_connection(mut stream: switchy_tcp::TokioTcpStream) -> Result<(), Box<dyn Error>> {
    // Get connection information
    let local_addr = stream.local_addr()?;
    let peer_addr = stream.peer_addr()?;

    println!("Connection established: {local_addr} (local) <-> {peer_addr} (remote)");

    // Buffer for reading data
    let mut buffer = [0u8; 1024];

    loop {
        // Read data from the client
        let bytes_read = stream.read(&mut buffer).await?;

        // If 0 bytes were read, the client has closed the connection
        if bytes_read == 0 {
            break;
        }

        // Display what we received (as UTF-8 if possible)
        let received = String::from_utf8_lossy(&buffer[..bytes_read]);
        println!("Received from {peer_addr}: {received:?}");

        // Echo the data back to the client
        stream.write_all(&buffer[..bytes_read]).await?;
        println!("Echoed {bytes_read} bytes back to {peer_addr}");
    }

    Ok(())
}
