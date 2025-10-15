# MoosicBox Native App Bundled Service

Service management for bundled MoosicBox native applications with embedded server.

## Overview

The MoosicBox Native App Bundled Service provides:

- **Embedded Server**: Built-in MoosicBox server for standalone applications
- **Service Management**: Async service lifecycle management
- **Event Handling**: Tauri application event processing
- **Startup Coordination**: Server startup synchronization
- **Graceful Shutdown**: Clean application and server termination

## Features

### Service Management

- **Async Service**: Built on MoosicBox async service framework
- **Command Processing**: Event-driven command handling
- **Context Management**: Shared application context and state
- **Error Handling**: Comprehensive error management

### Embedded Server

- **Basic Server**: MoosicBox server running on 0.0.0.0:8016
- **App Configuration**: Configured for native app usage
- **Auto-Start**: Automatic server startup with application
- **Background Operation**: Non-blocking server execution

### Event Integration

- **Tauri Events**: Native Tauri application event handling
- **Exit Handling**: Clean application exit processing (ExitRequested triggers server shutdown)
- **Event Forwarding**: Receives lifecycle events (Exit, WindowEvent, Ready, Resumed, MainEventsCleared)

### Coordination

- **Startup Sync**: Wait for server startup completion
- **Shutdown Sync**: Coordinated application and server shutdown
- **State Management**: Application and server state coordination

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_native_bundled = { path = "../app/native/bundled" }
```

## Usage

### Service Creation and Management

```rust
use moosicbox_app_native_bundled::{service, Command, Context};
use moosicbox_async_service::runtime::Handle;

// Create service context
let handle = Handle::current();
let context = Context::new(&handle);

// Create and start service
let service = service::Service::new(context);
service.start().await?;

// Send commands
service.send(Command::WaitForStartup { sender }).await?;
```

### Event Handling

```rust
use moosicbox_app_native_bundled::Command;
use tauri::RunEvent;
use std::sync::Arc;

// Handle Tauri events
let event = Arc::new(RunEvent::ExitRequested { api: exit_api });
service.send(Command::RunEvent { event }).await?;
```

### Startup Synchronization

```rust
// Wait for server startup
let (sender, receiver) = switchy_async::sync::oneshot::channel();
service.send(Command::WaitForStartup { sender }).await?;

// Wait for startup completion
receiver.await?;
println!("Server is ready!");
```

### Shutdown Coordination

```rust
// Wait for clean shutdown
let (sender, receiver) = switchy_async::sync::oneshot::channel();
service.send(Command::WaitForShutdown { sender }).await?;

// Wait for shutdown completion
receiver.await?;
println!("Application shutdown complete");
```

## Commands

### Available Commands

- **RunEvent**: Process Tauri application events
- **WaitForStartup**: Wait for server startup completion
- **WaitForShutdown**: Wait for application shutdown

### Command Processing

All commands are processed asynchronously through the service framework with:

- Error handling and logging
- State management
- Response coordination

## Context Management

### Context Structure

- **server_handle**: Background server task handle
- **receiver**: Startup synchronization receiver
- **Event handling**: Tauri event processing logic

### Server Configuration

- **Address**: 0.0.0.0 (all interfaces)
- **Port**: 8016 (fixed port for native apps)
- **Type**: App configuration for native use
- **Background**: Non-blocking execution

## Event Handling

### Supported Tauri Events

- **Exit**: Application exit events
- **ExitRequested**: User-initiated exit requests (triggers server shutdown)
- **WindowEvent**: Window-specific events
- **Ready**: Application ready state
- **Resumed**: Application resume events
- **MainEventsCleared**: Main event loop cleared

### Event Processing

- Automatic server shutdown on exit requests (ExitRequested event)
- Other events are received but not actively processed

## Error Handling

Comprehensive error management for:

- **Service Errors**: Service framework operation failures
- **IO Errors**: Server startup and shutdown failures
- **Command Errors**: Command processing failures
- **Event Errors**: Tauri event handling failures

## Dependencies

- **MoosicBox Async Service**: Service framework foundation
- **MoosicBox Server**: Embedded server functionality
- **MoosicBox Config**: Application type configuration
- **MoosicBox Assert**: Assertion utilities
- **Tauri**: Native application framework
- **switchy_async**: Async runtime abstraction and synchronization
- **log**: Logging facade
- **strum/strum_macros**: String enum conversions
- **thiserror**: Error type derivation

## Integration

This package is designed for:

- **Native Desktop Apps**: Standalone desktop applications
- **Bundled Deployments**: Self-contained application distributions
- **Embedded Servers**: Applications with built-in server functionality
- **Cross-Platform Apps**: Multi-platform native applications
