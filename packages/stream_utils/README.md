# MoosicBox Stream Utils

Basic byte stream utilities for simple data streaming in the MoosicBox ecosystem.

## Overview

The MoosicBox Stream Utils package provides:

- **ByteWriter/ByteStream**: Simple byte writer that can create multiple byte streams
- **TypedWriter/TypedStream**: Generic writer/stream utilities for typed data
- **Stalled Monitor**: Optional stalled read monitoring for streams (feature-gated)
- **Remote ByteStream**: Optional remote byte stream support (feature-gated)

## Features

### Core Components

- **ByteWriter**: Write bytes to multiple stream receivers
- **ByteStream**: Async stream that receives bytes from a ByteWriter
- **TypedWriter<T>**: Generic writer for any cloneable type T
- **TypedStream<T>**: Generic stream for receiving typed data

### Optional Features

- **Stalled Monitor**: Monitor streams for stalled reads (`stalled-monitor` feature, enabled by default)
- **Remote ByteStream**: Support for remote byte streaming (`remote-bytestream` feature, enabled by default)

## Installation

### Cargo Dependencies

```toml
[dependencies]
moosicbox_stream_utils = { path = "../stream_utils" }

# Optional: Enable specific features
moosicbox_stream_utils = {
    path = "../stream_utils",
    features = ["stalled-monitor", "remote-bytestream"]
}
```

## Usage

### Basic ByteWriter/ByteStream

```rust
use moosicbox_stream_utils::{ByteWriter, ByteStream};
use std::io::Write;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a byte writer
    let mut writer = ByteWriter::default();

    // Create streams from the writer
    let stream1 = writer.stream();
    let stream2 = writer.stream();

    // Write data to the writer
    writer.write_all(b"Hello, world!")?;
    writer.close();

    // Read from the streams (ByteStream yields Result<Bytes, std::io::Error>)
    let data1: Vec<_> = stream1.collect::<Vec<_>>().await;
    let data2: Vec<_> = stream2.collect::<Vec<_>>().await;

    println!("Stream 1 received {} chunks", data1.len());
    println!("Stream 2 received {} chunks", data2.len());

    Ok(())
}
```

### Typed Writer/Stream

```rust
use moosicbox_stream_utils::{TypedWriter, TypedStream};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a typed writer for strings
    let writer = TypedWriter::<String>::default();
    let mut stream = writer.stream();

    // Write some data
    writer.write("Hello".to_string());
    writer.write("World".to_string());

    // Read from the stream
    while let Some(data) = stream.next().await {
        println!("Received: {}", data);
    }

    Ok(())
}
```

### Stalled Read Monitoring (Optional)

```rust
#[cfg(feature = "stalled-monitor")]
use moosicbox_stream_utils::ByteWriter;
use std::time::Duration;
use futures::StreamExt;

#[cfg(feature = "stalled-monitor")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    // Add stalled read monitoring with timeout and optional throttle
    let mut monitored_stream = stream
        .stalled_monitor()
        .with_timeout(Duration::from_secs(5))
        .with_throttle(Duration::from_millis(100));

    // Use the monitored stream - will error with TimedOut if no data received within timeout
    while let Some(result) = monitored_stream.next().await {
        match result {
            Ok(bytes_result) => {
                // Handle the bytes (note: ByteStream yields Result<Bytes, std::io::Error>)
                let bytes = bytes_result?;
                println!("Received {} bytes", bytes.len());
            }
            Err(e) => {
                eprintln!("Stalled or timed out: {}", e);
                break;
            }
        }
    }

    Ok(())
}
```

### Remote Byte Stream (Optional)

```rust
#[cfg(feature = "remote-bytestream")]
use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use std::io::{Read, Seek, SeekFrom};
use switchy_async::util::CancellationToken;

#[cfg(feature = "remote-bytestream")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let abort = CancellationToken::new();

    // Create a remote byte stream that fetches data from an HTTP URL on demand
    let mut stream = RemoteByteStream::new(
        "https://example.com/file.mp3".to_string(),
        Some(1000000), // Optional: total file size in bytes (required for seeking from end)
        true,          // Auto-start fetching
        true,          // Seekable
        abort,
    );

    // Read data like a normal stream
    let mut buffer = [0u8; 1024];
    let bytes_read = stream.read(&mut buffer)?;
    println!("Read {} bytes", bytes_read);

    // Seek to a different position (triggers new HTTP range request if needed)
    stream.seek(SeekFrom::Start(50000))?;
    let bytes_read = stream.read(&mut buffer)?;
    println!("Read {} bytes from position 50000", bytes_read);

    Ok(())
}
```

## Configuration

The stream utilities support some basic configuration:

- **Writer ID**: Each writer gets a unique ID for tracking
- **Buffer Management**: Automatic cleanup of disconnected receivers
- **Error Handling**: Graceful handling of disconnected streams

## Error Handling

The utilities handle common error scenarios:

- **Disconnected Receivers**: Automatically removed from writer
- **Write Failures**: Logged and handled gracefully
- **Stream Completion**: Proper cleanup when streams end

## Implementation Notes

- Writers use unbounded channels internally
- Multiple streams can be created from a single writer
- All data written to a writer is cloned to all active streams
- Writers track total bytes written
- Streams are async and implement the `futures::Stream` trait
