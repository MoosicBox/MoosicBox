#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Stalled monitoring example demonstrating timeout and throttling.
//!
//! This example shows how to use `StalledReadMonitor` to add timeout detection
//! and throttling to streams, preventing hangs and controlling consumption rate.

use std::io::Write;
use std::time::Duration;

use futures::StreamExt;
use moosicbox_stream_utils::ByteWriter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Stalled Monitoring Example ===\n");

    // Example 1: Timeout detection
    println!("--- Example 1: Timeout Detection ---\n");
    timeout_example().await?;

    println!("\n--- Example 2: Throttling ---\n");
    throttle_example().await?;

    println!("\n--- Example 3: Combined Timeout and Throttling ---\n");
    combined_example().await?;

    println!("\n=== Example Complete ===");
    println!("\nKey takeaways:");
    println!("- StalledReadMonitor wraps streams to add timeout/throttling");
    println!("- Timeouts prevent indefinite hangs when data stops flowing");
    println!("- Throttling controls the rate of data consumption");
    println!("- Both can be combined for comprehensive flow control");
    println!("- Monitored streams yield Result<T> to report timeout errors");

    Ok(())
}

/// Demonstrates timeout detection when a stream stalls
async fn timeout_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Wrap the stream with a stalled monitor with a 2-second timeout
    let mut monitored = stream
        .stalled_monitor()
        .with_timeout(Duration::from_secs(2));

    println!("Created ByteStream with 2-second timeout");
    println!("Writing initial data...\n");

    // Write some initial data
    writer.write_all(b"Initial data")?;

    // Read the first chunk successfully
    if let Some(result) = monitored.next().await {
        match result {
            Ok(bytes_result) => {
                let bytes = bytes_result?;
                println!(
                    "✓ Received: {} bytes - '{}'",
                    bytes.len(),
                    String::from_utf8_lossy(&bytes)
                );
            }
            Err(e) => {
                eprintln!("✗ Monitor error: {e}");
            }
        }
    }

    println!("\nWaiting 3 seconds before next write (will exceed timeout)...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Write more data (but reader will have timed out)
    writer.write_all(b"Late data")?;

    // Try to read - should timeout
    println!("\nAttempting to read...");
    if let Some(result) = monitored.next().await {
        match result {
            Ok(bytes_result) => {
                let bytes = bytes_result?;
                println!(
                    "✓ Received: {} bytes - '{}'",
                    bytes.len(),
                    String::from_utf8_lossy(&bytes)
                );
            }
            Err(e) => {
                eprintln!("✗ Stream timed out as expected: {e}");
                eprintln!("   (No data received within 2-second timeout)");
            }
        }
    }

    Ok(())
}

/// Demonstrates throttling to control consumption rate
async fn throttle_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Wrap the stream with a 500ms throttle
    let mut monitored = stream
        .stalled_monitor()
        .with_throttle(Duration::from_millis(500));

    println!("Created ByteStream with 500ms throttle");
    println!("Writing multiple chunks rapidly...\n");

    // Spawn a task to consume the stream with throttling
    let reader_handle = tokio::spawn(async move {
        let mut count = 0;
        let start = tokio::time::Instant::now();

        while let Some(result) = monitored.next().await {
            match result {
                Ok(bytes_result) => {
                    let bytes = bytes_result.map_err(|e| e.to_string())?;
                    if bytes.is_empty() {
                        println!("Received close signal");
                        break;
                    }
                    count += 1;
                    let elapsed = start.elapsed();
                    println!(
                        "[{:>6}ms] Chunk {}: {} bytes - '{}'",
                        elapsed.as_millis(),
                        count,
                        bytes.len(),
                        String::from_utf8_lossy(&bytes)
                    );
                }
                Err(e) => {
                    eprintln!("Monitor error: {e}");
                    break;
                }
            }
        }

        let total_time = start.elapsed();
        println!("\nReceived {count} chunks in {total_time:?}");
        println!("Average time per chunk: ~{:?}", total_time / count);

        Ok::<_, String>(())
    });

    // Give the reader a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write chunks as fast as possible
    for i in 1..=5 {
        writer.write_all(format!("Message {i}").as_bytes())?;
        println!("[Writer] Wrote message {i}");
    }

    println!("\n[Writer] All messages written, closing stream");
    writer.close();

    // Wait for reader to finish
    reader_handle
        .await
        .map_err(|e| format!("Join error: {e}"))?
        .map_err(|e| format!("Task error: {e}"))?;

    println!("\nNotice: Messages are consumed at ~500ms intervals despite rapid writing");

    Ok(())
}

/// Demonstrates combining timeout and throttling
async fn combined_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ByteWriter::default();
    let stream = writer.stream();

    // Wrap with both timeout and throttle
    let mut monitored = stream
        .stalled_monitor()
        .with_timeout(Duration::from_secs(3))
        .with_throttle(Duration::from_millis(300));

    println!("Created ByteStream with 3-second timeout AND 300ms throttle");
    println!("Writing data at irregular intervals...\n");

    let reader_handle = tokio::spawn(async move {
        let start = tokio::time::Instant::now();
        let mut count = 0;

        loop {
            match monitored.next().await {
                Some(Ok(bytes_result)) => {
                    let bytes = bytes_result.map_err(|e| e.to_string())?;
                    if bytes.is_empty() {
                        println!(
                            "[{:>6}ms] Received close signal",
                            start.elapsed().as_millis()
                        );
                        break;
                    }
                    count += 1;
                    println!(
                        "[{:>6}ms] ✓ Received: '{}'",
                        start.elapsed().as_millis(),
                        String::from_utf8_lossy(&bytes)
                    );
                }
                Some(Err(e)) => {
                    eprintln!("[{:>6}ms] ✗ Timed out: {e}", start.elapsed().as_millis());
                    break;
                }
                None => {
                    println!("[{:>6}ms] Stream ended", start.elapsed().as_millis());
                    break;
                }
            }
        }

        println!("\nSuccessfully received {count} messages before completion/timeout");

        Ok::<_, String>(())
    });

    // Give the reader a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write data with varying delays
    writer.write_all(b"Message 1 (immediate)")?;
    println!("[Writer] Wrote message 1");

    tokio::time::sleep(Duration::from_millis(400)).await;
    writer.write_all(b"Message 2 (after 400ms)")?;
    println!("[Writer] Wrote message 2");

    tokio::time::sleep(Duration::from_millis(500)).await;
    writer.write_all(b"Message 3 (after 500ms)")?;
    println!("[Writer] Wrote message 3");

    tokio::time::sleep(Duration::from_millis(400)).await;
    writer.write_all(b"Message 4 (after 400ms)")?;
    println!("[Writer] Wrote message 4");

    // Now wait longer than the timeout (3+ seconds)
    println!("\n[Writer] Waiting 4 seconds (will exceed timeout)...");
    tokio::time::sleep(Duration::from_secs(4)).await;

    writer.write_all(b"Message 5 (too late)")?;
    println!("[Writer] Wrote message 5 (but reader already timed out)");

    writer.close();

    reader_handle
        .await
        .map_err(|e| format!("Join error: {e}"))?
        .map_err(|e| format!("Task error: {e}"))?;

    println!("\nNotice: Throttling slowed consumption, but timeout still triggered");

    Ok(())
}
