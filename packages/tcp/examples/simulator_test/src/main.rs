#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! In-memory TCP simulator example using `switchy_tcp`.
//!
//! This example demonstrates:
//! - Using the TCP simulator for testing without real networking
//! - Simulating client-server communication in-memory
//! - Testing TCP code deterministically
//! - Understanding simulator features like port allocation and DNS

use std::error::Error;

use switchy_tcp::{GenericTcpListener, TcpListener, TcpStream, simulator};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Switchy TCP Simulator Example ===\n");

    // Run basic echo test
    println!("1. Testing basic echo server...");
    test_echo_server().await?;

    // Demonstrate port allocation
    println!("\n2. Demonstrating ephemeral port allocation...");
    demonstrate_port_allocation().await?;

    // Demonstrate stream splitting
    println!("\n3. Demonstrating stream splitting...");
    demonstrate_stream_splitting().await?;

    println!("\n=== All tests completed successfully! ===");

    Ok(())
}

/// Tests a simple echo server using the simulator.
async fn test_echo_server() -> Result<(), Box<dyn Error>> {
    // Bind a listener using the simulator
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("   Simulator listener bound to 127.0.0.1:8080");

    // Spawn a task to accept and handle one connection
    let server_task = tokio::spawn(async move {
        let (mut stream, addr) = listener
            .accept()
            .await
            .expect("Failed to accept connection");
        println!("   Server accepted connection from: {addr}");

        // Echo back whatever we receive
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).await.expect("Failed to read");
        println!("   Server received {n} bytes");

        stream
            .write_all(&buffer[..n])
            .await
            .expect("Failed to write");
        println!("   Server echoed data back");
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Connect a client
    let mut client = TcpStream::connect("127.0.0.1:8080").await?;
    println!("   Client connected to server");

    // Send test data
    let test_data = b"Hello from simulator!";
    client.write_all(test_data).await?;
    println!("   Client sent: {:?}", String::from_utf8_lossy(test_data));

    // Read the echo
    let mut buffer = [0u8; 1024];
    let n = client.read(&mut buffer).await?;
    println!(
        "   Client received echo: {:?}",
        String::from_utf8_lossy(&buffer[..n])
    );

    // Verify the data matches
    assert_eq!(&buffer[..n], test_data);
    println!("   ✓ Echo data verified!");

    // Wait for server task to complete
    server_task.await?;

    Ok(())
}

/// Demonstrates ephemeral port allocation in the simulator.
async fn demonstrate_port_allocation() -> Result<(), Box<dyn Error>> {
    // Reset the port counter for predictable output
    simulator::reset_next_port();

    let start_port = simulator::ephemeral_port_start();
    println!("   Ephemeral ports start at: {start_port}");

    // Bind to a specific port
    let _listener1 = TcpListener::bind("127.0.0.1:9000").await?;
    println!("   Bound listener to specified port: 9000");

    // The simulator automatically assigns ephemeral ports for client connections
    // when they connect, similar to real TCP behavior

    // Get some ephemeral ports
    let port1 = simulator::next_port();
    let port2 = simulator::next_port();
    let port3 = simulator::next_port();

    println!("   Next ephemeral ports: {port1}, {port2}, {port3}");
    println!("   ✓ Port allocation working!");

    Ok(())
}

/// Demonstrates splitting a TCP stream into read and write halves.
async fn demonstrate_stream_splitting() -> Result<(), Box<dyn Error>> {
    use switchy_tcp::GenericTcpStream;

    // Reset for clean state
    simulator::reset_next_port();

    // Set up server
    let listener = TcpListener::bind("127.0.0.1:8081").await?;

    // Server task: receive from read half, send on write half
    let server_task = tokio::spawn(async move {
        let (stream, _addr) = listener.accept().await.expect("Failed to accept");

        // Split the stream
        let (mut read_half, mut write_half) = stream.into_split();

        // Read from one half
        let mut buffer = [0u8; 1024];
        let n = read_half.read(&mut buffer).await.expect("Failed to read");

        // Write to the other half
        write_half
            .write_all(&buffer[..n])
            .await
            .expect("Failed to write");
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Client
    let mut client = TcpStream::connect("127.0.0.1:8081").await?;

    client.write_all(b"Split stream test").await?;
    println!("   Client sent data");

    let mut buffer = [0u8; 1024];
    let n = client.read(&mut buffer).await?;
    println!(
        "   Client received: {:?}",
        String::from_utf8_lossy(&buffer[..n])
    );

    println!("   ✓ Stream splitting works!");

    server_task.await?;

    Ok(())
}
