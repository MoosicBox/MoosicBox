# Basic Server Example

This example demonstrates how to start a basic MoosicBox server with default configuration settings. It shows the minimal code required to get a fully functional music server running.

## What This Example Demonstrates

- Starting a MoosicBox server using the `run_basic` function
- Configuring the server to listen on all network interfaces
- Initializing logging to see server activity
- Understanding the startup sequence and available endpoints
- Graceful shutdown handling with Ctrl+C

## Prerequisites

- Rust toolchain installed (1.70 or later)
- Basic understanding of async Rust and tokio
- Familiarity with HTTP servers and REST APIs (helpful but not required)

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/server/examples/basic_server/Cargo.toml
```

The server will start and listen on `http://localhost:8080`. You should see output similar to:

```
Starting MoosicBox server...
The server will be accessible at http://localhost:8080
Press Ctrl+C to stop the server
✓ Server started successfully!

Available endpoints:
  - Health check: http://localhost:8080/health
  - WebSocket: ws://localhost:8080/ws
  - API docs: http://localhost:8080/openapi (if openapi feature enabled)

Server is now running and ready to accept connections...
MoosicBox Server started on 192.168.1.100:8080
```

## Expected Output

When the server starts successfully, you'll see:

1. **Initialization logs**: Database setup, configuration loading
2. **Service startup messages**: WebSocket server, track pool, audio zones
3. **Network information**: Local IP address and port number
4. **Ready message**: Confirmation that the server is accepting connections

The server will continue running until you press Ctrl+C, which triggers a graceful shutdown.

## Code Walkthrough

### Main Function Setup

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    moosicbox_logging::init("basic_server_example")
        .expect("Failed to initialize logging");
```

The example uses the `tokio` async runtime and initializes logging first so all server activities are visible in the console.

### Starting the Server

```rust
moosicbox_server::run_basic(
    AppType::App,        // Standard application server type
    "0.0.0.0",          // Bind to all network interfaces
    8080,               // Port number
    None,               // Use default worker threads
    |handle| {          // Startup callback
        println!("✓ Server started successfully!");
        handle
    }
).await
```

The `run_basic` function is a simplified version of the full `run` function that uses sensible defaults:

- **SQLite database**: Configuration stored in `~/.local/share/moosicbox/` (Linux/macOS) or `AppData\Local\moosicbox\` (Windows)
- **All APIs enabled**: Audio output, sessions, library, player, etc.
- **All audio formats supported**: FLAC, MP3, AAC, OPUS
- **WebSocket support**: Real-time updates to connected clients
- **Default telemetry**: Basic request metrics (if feature enabled)

### Network Binding

The server binds to `"0.0.0.0"` which means:

- Accessible from `localhost` (127.0.0.1)
- Accessible from the local network (e.g., 192.168.1.100)
- Can be accessed by other devices on the same network

To restrict access to localhost only, change `"0.0.0.0"` to `"127.0.0.1"`.

## Key Concepts

### AppType

The `AppType::App` parameter indicates this is a standard application server. Other types include:

- `AppType::Test`: For testing environments
- Different types may affect default paths and behaviors

### run_basic vs run

This example uses `run_basic` which provides a streamlined interface. The full `run` function offers additional configuration options:

- Custom TCP listener
- Enable/disable specific features (local players, UPnP players)
- Custom metrics handlers
- More granular control over initialization

For most use cases, `run_basic` is the recommended starting point.

### Server Handle

The startup callback receives a `ServerHandle` which can be used to:

- Stop the server programmatically
- Query server state
- Monitor server health

In this example, we simply return it to let the runtime manage the server lifecycle.

## Testing the Example

### 1. Check Server Health

Open a browser or use curl to verify the server is running:

```bash
curl http://localhost:8080/health
```

Expected response: `{"status":"ok"}` or similar health status information.

### 2. Connect to WebSocket

Use a WebSocket client (like `websocat` or browser developer tools):

```bash
# Install websocat: cargo install websocat
websocat ws://localhost:8080/ws
```

The WebSocket connection enables real-time updates for playback state, session changes, and audio zone events.

### 3. Access from Another Device

Find your local IP address:

```bash
# Linux/macOS
ip addr show  # or: ifconfig

# Windows
ipconfig
```

Then access from another device on the same network:

```
http://192.168.1.100:8080/health
```

### 4. Stop the Server

Press `Ctrl+C` in the terminal. The server will:

1. Stop accepting new connections
2. Complete in-flight requests
3. Shut down active players
4. Close database connections
5. Clean up resources
6. Exit cleanly

## Troubleshooting

### Port Already in Use

**Error**: `Address already in use (os error 98)` or similar

**Solution**: Another application is using port 8080. Either:

- Stop the other application
- Change the port number in the example (e.g., use 8081 instead of 8080)

### Database Initialization Failed

**Error**: `Failed to initialize database` or `Failed to migrate database`

**Solution**: Ensure you have write permissions to the configuration directory:

- Linux/macOS: `~/.local/share/moosicbox/`
- Windows: `%LOCALAPPDATA%\moosicbox\`

Create the directory manually if needed:

```bash
mkdir -p ~/.local/share/moosicbox/
```

### Cannot Access from Other Devices

**Issue**: Server works on localhost but not from network

**Solution**: Check firewall settings:

```bash
# Linux (using ufw)
sudo ufw allow 8080/tcp

# macOS
# System Preferences → Security & Privacy → Firewall → Firewall Options

# Windows
# Windows Defender Firewall → Advanced settings → Inbound Rules → New Rule
```

## Related Examples

- `packages/async_service/examples/basic_service/` - Understanding the underlying service pattern used by the server
- `packages/admin_htmx/examples/basic_admin_server/` - Server with admin UI interface
- `packages/web_server/examples/simple_get/` - Simple HTTP server patterns in MoosicBox

For more advanced server configuration, see the main `moosicbox_server` documentation and explore the full `run` function API.
