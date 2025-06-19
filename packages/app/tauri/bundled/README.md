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

- **Basic Server**: MoosicBox server running on localhost:8016
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

### Event Handling

## Dependencies

- **MoosicBox Async Service**: Service framework foundation
- **MoosicBox Server**: Embedded server functionality
- **MoosicBox Downloader**: Download management
- **MoosicBox Scan**: Library scanning integration
- **MoosicBox Profiles**: Multi-profile support
- **Tauri**: Desktop application framework
- **Tokio**: Async runtime and synchronization
