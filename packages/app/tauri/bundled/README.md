# MoosicBox App Tauri Bundled

Tauri bundled application service for MoosicBox that runs the MoosicBox server embedded within a desktop application.

## Features

- **Embedded Server**: Runs the full MoosicBox server within the Tauri application
- **Lifecycle Management**: Handles server startup, shutdown, and event processing
- **Async Service Architecture**: Uses async service patterns for command processing
- **Download Path Management**: Automatically configures download directories as scan paths
- **Profile Support**: Responds to profile creation events to set up scan paths

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_tauri_bundled = "0.1.4"
```

## Usage

```rust
use moosicbox_app_tauri_bundled::{Context, service};

fn main() {
    let runtime_handle = moosicbox_async_service::runtime::Handle::current();
    let ctx = Context::new(&runtime_handle);
    let service = service::Service::new(ctx);
    let _handle = service.start();
}
```

### Handling Tauri Events

```rust
use moosicbox_app_tauri_bundled::{Command, service};
use moosicbox_async_service::Arc;
use tauri::RunEvent;

fn handle_run_event(handle: &service::Handle, event: RunEvent) {
    let cmd = Command::RunEvent { event: Arc::new(event) };
    handle.send_command(cmd).expect("Failed to send command");
}
```

### Waiting for Server Startup

```rust
use moosicbox_app_tauri_bundled::Command;
use switchy_async::sync::oneshot;

async fn wait_for_startup(handle: &moosicbox_app_tauri_bundled::service::Handle) {
    let (sender, receiver) = oneshot::channel();
    handle.send_command(Command::WaitForStartup { sender }).expect("Failed to send command");
    receiver.await.expect("Failed to wait for startup");
}
```

## API

### Context

The `Context` struct manages the embedded server's runtime state.

- `Context::new(handle)`: Creates a new context and starts the embedded server on `0.0.0.0:8016`
- `handle_event(event)`: Processes Tauri run events, initiating shutdown on exit requests
- `shutdown()`: Shuts down the embedded server

### Command

Commands for controlling the service lifecycle:

- `RunEvent`: Process a Tauri run event
- `WaitForStartup`: Wait for the server to complete startup
- `WaitForShutdown`: Wait for the server to complete shutdown

### service Module

The `service` module provides async service infrastructure:

- `Service`: Main service struct for lifecycle management
- `Handle`: Cloneable handle for sending commands to the service
- `Processor`: Trait for command processing

## Cargo Features

- `tunnel`: Enable tunnel support for the embedded server
- `decoder-aac`: Enable AAC audio decoding
- `decoder-flac`: Enable FLAC audio decoding
- `decoder-mp3`: Enable MP3 audio decoding
- `format-aac`: Enable AAC format support
- `format-flac`: Enable FLAC format support
- `format-mp3`: Enable MP3 format support
- `fail-on-warnings`: Treat warnings as errors during compilation

## License

See the [LICENSE](../../../../LICENSE) file for details.
