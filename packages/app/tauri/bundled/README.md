# MoosicBox Tauri Bundled Service

Service management for bundled MoosicBox Tauri applications with embedded server.

## Overview

The MoosicBox Tauri Bundled Service provides:

- **Embedded Server**: Built-in MoosicBox server for Tauri applications
- **Service Management**: Async service lifecycle management
- **Event Handling**: Tauri application event processing
- **Download Management**: Automatic download location setup
- **Profile Integration**: Multi-profile support with automatic configuration

## Features

### Service Management

- **Async Service**: Built on MoosicBox async service framework
- **Command Processing**: Event-driven command handling
- **Context Management**: Shared application context and state
- **Error Handling**: Comprehensive error management

### Embedded Server

- **Basic Server**: MoosicBox server running on 0.0.0.0:8016
- **App Configuration**: Configured for Tauri app usage
- **Auto-Start**: Automatic server startup with application
- **Background Operation**: Non-blocking server execution

### Download Integration

- **Download Paths**: Automatic download location creation
- **Scan Integration**: Download paths added to library scan
- **Profile Support**: Per-profile download configuration
- **Event Handling**: Profile change event processing

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_tauri_bundled = { path = "../app/tauri/bundled" }
```

## Usage

### Service Creation and Management

```rust,no_run
use moosicbox_app_tauri_bundled::{service::Service, Context};
use moosicbox_async_service::runtime::Handle;

// Create the service context
let handle = Handle::current();
let context = Context::new(&handle);

// Create and start the service
let service = Service::new(context);
let service_handle = service.handle();
let _join_handle = service.start();

// Send commands via the service handle (example - requires sender in scope)
# /*
service_handle.send_command_async(Command::WaitForStartup { sender }).await?;
# */
```

### Event Handling

The service processes commands through an async service framework:

```rust,no_run
use moosicbox_app_tauri_bundled::Command;
use moosicbox_async_service::Arc;
# use switchy_async::sync::oneshot;
# use tauri::RunEvent;
#
# async fn example_event_handling(service_handle: moosicbox_app_tauri_bundled::service::Handle, run_event: RunEvent) -> Result<(), Box<dyn std::error::Error>> {

// Handle Tauri run events
let event = Arc::new(run_event);
service_handle.send_command_async(Command::RunEvent { event }).await?;

// Wait for server startup
let (sender, receiver) = oneshot::channel();
service_handle.send_command_async(Command::WaitForStartup { sender }).await?;
receiver.await?;

// Wait for server shutdown
let (sender, receiver) = oneshot::channel();
service_handle.send_command_async(Command::WaitForShutdown { sender }).await?;
receiver.await?;

# Ok(())
# }
```

## Dependencies

### Core Dependencies

- **moosicbox_async_service**: Service framework foundation
- **moosicbox_server**: Embedded server functionality (with `app-apis` and `sqlite-sqlx` features)
- **moosicbox_config**: Application configuration
- **moosicbox_downloader**: Download management
- **moosicbox_scan**: Library scanning integration
- **moosicbox_profiles**: Multi-profile support (with `events` feature)
- **moosicbox_assert**: Assertion utilities
- **switchy_async**: Async utilities (with `sync` and `tokio` features)
- **switchy_database**: Database management
- **tauri**: Desktop application framework
- **log**: Logging facade
- **strum** / **strum_macros**: Enum utilities
- **thiserror**: Error handling

## Features

- **tunnel**: Enable server tunnel functionality
- **decoder-aac**: AAC audio decoder support
- **decoder-flac**: FLAC audio decoder support
- **decoder-mp3**: MP3 audio decoder support
- **format-aac**: AAC audio format support
- **format-flac**: FLAC audio format support
- **format-mp3**: MP3 audio format support
- **fail-on-warnings**: Treat warnings as errors in dependencies
