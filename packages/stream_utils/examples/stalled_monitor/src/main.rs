#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating stalled read monitoring for streams.
//!
//! This example shows how to use `StalledReadMonitor` to add timeout and
//! throttling capabilities to any stream, enabling detection of stalled
//! data flow and rate limiting.

use std::io::Write;
use std::time::Duration;

use futures::StreamExt;
use moosicbox_stream_utils::ByteWriter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Stalled Read Monitor Example ===\n");

    // Example 1: Basic timeout detection
    println!("--- Example 1: Timeout Detection ---\n");
    timeout_detection().await?;

    // Example 2: Throttling stream consumption
    println!("\n--- Example 2: Stream Throttling ---\n");
    stream_throttling().await?;

    // Example 3: Combined timeout and throttling
    println!("\n--- Example 3: Combined Timeout and Throttling ---\n");
    combined_monitoring().await?;

    Ok(())
}

async fn timeout_detection() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating a stream that will stall...");

    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Add timeout monitoring - will timeout if no data received within 2 seconds
    let mut monitored_stream = stream
        .stalled_monitor()
        .with_timeout(Duration::from_secs(2));

    println!("Created monitored stream with 2-second timeout\n");

    // Spawn a task to read from the stream
    let reader_handle = tokio::spawn(async move {
        let mut chunk_count = 0;

        while let Some(result) = monitored_stream.next().await {
            match result {
                Ok(bytes_result) => match bytes_result {
                    Ok(bytes) => {
                        if bytes.is_empty() {
                            println!("Reader: Received end signal");
                            break;
                        }
                        chunk_count += 1;
                        println!(
                            "Reader: Received chunk {} ({} bytes)",
                            chunk_count,
                            bytes.len()
                        );
                    }
                    Err(e) => {
                        eprintln!("Reader: Error from ByteStream: {e}");
                        break;
                    }
                },
                Err(e) => {
                    eprintln!("Reader: Monitor error - {e}");
                    if e.kind() == std::io::ErrorKind::TimedOut {
                        println!("Reader: Stream timed out after no data for 2 seconds!");
                        return Err("timeout");
                    }
                    break;
                }
            }
        }

        Ok("completed")
    });

    // Write some data
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Writer: Sending first chunk...");
    writer.write_all(b"First chunk")?;

    // Write more data within timeout
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("Writer: Sending second chunk...");
    writer.write_all(b"Second chunk")?;

    // Wait longer than timeout - this will cause the stream to timeout
    println!("Writer: Waiting 3 seconds (longer than 2-second timeout)...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    println!("Writer: Attempting to send third chunk (but reader should have timed out)...");
    writer.write_all(b"Third chunk")?;

    // Wait for reader to complete
    let result = reader_handle.await?;

    match result {
        Ok(msg) => println!("\n✓ Reader {msg}"),
        Err(msg) => println!("\n✓ Reader detected {msg} as expected"),
    }

    Ok(())
}

async fn stream_throttling() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating a throttled stream...");

    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Add throttling - will wait at least 500ms between reads
    let mut monitored_stream = stream
        .stalled_monitor()
        .with_throttle(Duration::from_millis(500));

    println!("Created throttled stream (500ms between reads)\n");

    // Spawn reader with timing measurements
    let reader_handle = tokio::spawn(async move {
        let mut last_time = tokio::time::Instant::now();
        let mut chunk_count = 0;

        while let Some(result) = monitored_stream.next().await {
            match result {
                Ok(bytes_result) => match bytes_result {
                    Ok(bytes) => {
                        if bytes.is_empty() {
                            println!("Reader: Received end signal");
                            break;
                        }
                        chunk_count += 1;
                        let now = tokio::time::Instant::now();
                        let elapsed = now.duration_since(last_time);
                        println!(
                            "Reader: Chunk {} received after {:.1}s",
                            chunk_count,
                            elapsed.as_secs_f64()
                        );
                        last_time = now;
                    }
                    Err(e) => {
                        eprintln!("Reader: ByteStream error: {e}");
                        break;
                    }
                },
                Err(e) => {
                    eprintln!("Reader: Monitor error: {e}");
                    break;
                }
            }
        }

        chunk_count
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write data rapidly - but reader will throttle consumption
    println!("Writer: Sending data rapidly...\n");
    for i in 1..=5 {
        writer.write_all(format!("Chunk {i}").as_bytes())?;
        println!("Writer: Sent chunk {i}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    writer.close();

    let chunks_read = reader_handle.await?;
    println!("\n✓ Throttling enforced: {chunks_read} chunks read with 500ms delays");

    Ok(())
}

async fn combined_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating stream with both timeout and throttling...");

    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Add both timeout (3 seconds) and throttling (200ms)
    let mut monitored_stream = stream
        .stalled_monitor()
        .with_timeout(Duration::from_secs(3))
        .with_throttle(Duration::from_millis(200));

    println!("Created stream with 3s timeout and 200ms throttle\n");

    // Spawn reader
    let reader_handle = tokio::spawn(async move {
        let start_time = tokio::time::Instant::now();
        let mut chunk_count = 0;

        while let Some(result) = monitored_stream.next().await {
            match result {
                Ok(bytes_result) => match bytes_result {
                    Ok(bytes) => {
                        if bytes.is_empty() {
                            println!("Reader: Received end signal");
                            break;
                        }
                        chunk_count += 1;
                        let elapsed = start_time.elapsed();
                        println!(
                            "Reader: Chunk {} at {:.1}s ({} bytes)",
                            chunk_count,
                            elapsed.as_secs_f64(),
                            bytes.len()
                        );
                    }
                    Err(e) => {
                        eprintln!("Reader: ByteStream error: {e}");
                        break;
                    }
                },
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::TimedOut {
                        eprintln!("Reader: Timed out after no data for 3 seconds");
                        return Err("timeout");
                    }
                    eprintln!("Reader: Monitor error: {e}");
                    return Err("error");
                }
            }
        }

        Ok(chunk_count)
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write data at various intervals
    println!("Writer: Sending chunks at different intervals...\n");

    writer.write_all(b"Chunk 1")?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    writer.write_all(b"Chunk 2")?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    writer.write_all(b"Chunk 3")?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    writer.write_all(b"Chunk 4")?;
    writer.close();

    // Wait for reader
    match reader_handle.await? {
        Ok(count) => {
            println!("\n✓ Successfully read {count} chunks with combined timeout and throttling");
        }
        Err(msg) => {
            println!("\n✗ Reader encountered {msg}");
        }
    }

    Ok(())
}
