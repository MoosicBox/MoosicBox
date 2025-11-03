#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

//! Basic broadcast example demonstrating `ByteWriter` and `ByteStream`.
//!
//! This example shows how to use `ByteWriter` to broadcast bytes to multiple
//! `ByteStream` readers, where each reader receives a copy of all written data.

use std::io::Write;

use futures::StreamExt;
use moosicbox_stream_utils::ByteWriter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic Broadcast Example ===\n");

    // Create a ByteWriter that will broadcast data to multiple streams
    let mut writer = ByteWriter::default();
    println!("Created ByteWriter with ID: {}", writer.id);

    // Create three streams that will each receive copies of the data
    let mut stream1 = writer.stream();
    let mut stream2 = writer.stream();
    let mut stream3 = writer.stream();
    println!("Created 3 ByteStream readers\n");

    // Spawn tasks to read from each stream concurrently
    let handle1 = tokio::spawn(async move {
        println!("[Stream 1] Starting to read...");
        let mut chunks = 0;
        let mut total_bytes = 0;

        while let Some(result) = stream1.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("[Stream 1] Received close signal");
                        break;
                    }
                    chunks += 1;
                    total_bytes += bytes.len();
                    println!(
                        "[Stream 1] Received chunk {}: {} bytes - '{}'",
                        chunks,
                        bytes.len(),
                        String::from_utf8_lossy(&bytes)
                    );
                }
                Err(e) => {
                    eprintln!("[Stream 1] Error: {e}");
                    break;
                }
            }
        }

        println!("[Stream 1] Finished: {chunks} chunks, {total_bytes} total bytes\n");
    });

    let handle2 = tokio::spawn(async move {
        println!("[Stream 2] Starting to read...");
        let mut chunks = 0;
        let mut total_bytes = 0;

        while let Some(result) = stream2.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("[Stream 2] Received close signal");
                        break;
                    }
                    chunks += 1;
                    total_bytes += bytes.len();
                    println!(
                        "[Stream 2] Received chunk {}: {} bytes",
                        chunks,
                        bytes.len()
                    );
                }
                Err(e) => {
                    eprintln!("[Stream 2] Error: {e}");
                    break;
                }
            }
        }

        println!("[Stream 2] Finished: {chunks} chunks, {total_bytes} total bytes\n");
    });

    let handle3 = tokio::spawn(async move {
        println!("[Stream 3] Starting to read...");
        let mut chunks = 0;
        let mut total_bytes = 0;

        while let Some(result) = stream3.next().await {
            match result {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        println!("[Stream 3] Received close signal");
                        break;
                    }
                    chunks += 1;
                    total_bytes += bytes.len();
                    println!(
                        "[Stream 3] Received chunk {}: {} bytes",
                        chunks,
                        bytes.len()
                    );
                }
                Err(e) => {
                    eprintln!("[Stream 3] Error: {e}");
                    break;
                }
            }
        }

        println!("[Stream 3] Finished: {chunks} chunks, {total_bytes} total bytes\n");
    });

    // Give the tasks a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Write some data to the writer - it will be broadcast to all streams
    println!("[Writer] Writing first message...");
    writer.write_all(b"Hello, World!")?;

    println!("[Writer] Writing second message...");
    writer.write_all(b"Broadcast message to all streams!")?;

    println!("[Writer] Writing third message...");
    writer.write_all(b"Final message before closing.")?;

    // Check how many bytes have been written
    println!("[Writer] Total bytes written: {}\n", writer.bytes_written());

    // Close the writer to signal streams that no more data will be sent
    println!("[Writer] Closing writer...\n");
    writer.close();

    // Wait for all reading tasks to complete
    handle1.await?;
    handle2.await?;
    handle3.await?;

    println!("=== Example Complete ===");
    println!("\nKey takeaways:");
    println!("- ByteWriter broadcasts data to multiple ByteStream readers");
    println!("- Each stream receives an independent copy of the data");
    println!("- writer.close() sends an empty bytes signal to indicate completion");
    println!("- ByteStream yields Result<Bytes, std::io::Error> items");

    Ok(())
}
