#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

//! Example demonstrating basic byte broadcasting with `ByteWriter` and `ByteStream`.
//!
//! This example shows how to use `ByteWriter` to write data once and broadcast
//! it to multiple `ByteStream` readers simultaneously.

use std::io::Write;

use futures::StreamExt;
use moosicbox_stream_utils::ByteWriter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic Byte Broadcasting Example ===\n");

    // Create a ByteWriter - this will broadcast data to multiple streams
    let mut writer = ByteWriter::default();
    println!("Created ByteWriter with ID: {}", writer.id);

    // Create multiple streams from the same writer
    // Each stream will receive a copy of all data written to the writer
    let stream1 = writer.stream();
    let stream2 = writer.stream();
    let stream3 = writer.stream();
    println!("Created 3 streams from the writer\n");

    // Spawn tasks to read from each stream concurrently
    let handle1 = tokio::spawn(async move {
        println!("Stream 1: Starting to read...");
        let mut chunks = Vec::new();
        let mut stream = stream1;

        // ByteStream yields Result<Bytes, std::io::Error>
        while let Some(result) = stream.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("Stream 1: Received end signal");
                        break;
                    }
                    println!("Stream 1: Received {} bytes", bytes.len());
                    chunks.push(bytes);
                }
                Err(e) => {
                    eprintln!("Stream 1: Error reading: {e}");
                    break;
                }
            }
        }

        // Combine all chunks into a single message
        let total_bytes: Vec<u8> = chunks.iter().flat_map(|b| b.iter().copied()).collect();
        let message = String::from_utf8_lossy(&total_bytes);
        println!("Stream 1: Complete message: {message:?}");
        message.to_string()
    });

    let handle2 = tokio::spawn(async move {
        println!("Stream 2: Starting to read...");
        let mut chunks = Vec::new();
        let mut stream = stream2;

        while let Some(result) = stream.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("Stream 2: Received end signal");
                        break;
                    }
                    println!("Stream 2: Received {} bytes", bytes.len());
                    chunks.push(bytes);
                }
                Err(e) => {
                    eprintln!("Stream 2: Error reading: {e}");
                    break;
                }
            }
        }

        let total_bytes: Vec<u8> = chunks.iter().flat_map(|b| b.iter().copied()).collect();
        let message = String::from_utf8_lossy(&total_bytes);
        println!("Stream 2: Complete message: {message:?}");
        message.to_string()
    });

    let handle3 = tokio::spawn(async move {
        println!("Stream 3: Starting to read...");
        let mut chunks = Vec::new();
        let mut stream = stream3;

        while let Some(result) = stream.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("Stream 3: Received end signal");
                        break;
                    }
                    println!("Stream 3: Received {} bytes", bytes.len());
                    chunks.push(bytes);
                }
                Err(e) => {
                    eprintln!("Stream 3: Error reading: {e}");
                    break;
                }
            }
        }

        let total_bytes: Vec<u8> = chunks.iter().flat_map(|b| b.iter().copied()).collect();
        let message = String::from_utf8_lossy(&total_bytes);
        println!("Stream 3: Complete message: {message:?}");
        message.to_string()
    });

    // Give the tasks a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Write data to the writer - it will be broadcast to all streams
    println!("\nWriting data to ByteWriter...");
    writer.write_all(b"Hello, ")?;
    println!("Wrote: \"Hello, \"");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write_all(b"streaming ")?;
    println!("Wrote: \"streaming \"");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    writer.write_all(b"world!")?;
    println!("Wrote: \"world!\"");

    // Close the writer to signal all streams that no more data will be sent
    println!("\nClosing the writer...");
    writer.close();

    // Wait for all streams to finish reading
    println!("\nWaiting for all streams to complete...\n");
    let result1 = handle1.await?;
    let result2 = handle2.await?;
    let result3 = handle3.await?;

    // Verify all streams received the same data
    println!("\n=== Results ===");
    println!("Stream 1 received: {result1:?}");
    println!("Stream 2 received: {result2:?}");
    println!("Stream 3 received: {result3:?}");

    assert_eq!(result1, "Hello, streaming world!");
    assert_eq!(result2, "Hello, streaming world!");
    assert_eq!(result3, "Hello, streaming world!");

    println!("\n✓ All streams received the same data successfully!");
    println!("✓ Total bytes written: {}", writer.bytes_written());

    Ok(())
}
