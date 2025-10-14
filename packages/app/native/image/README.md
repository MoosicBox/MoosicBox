# MoosicBox Native App Image Assets

Embedded image asset management for MoosicBox native applications.

## Overview

The MoosicBox Native App Image package provides:

- **Asset Embedding**: Compile-time image asset embedding
- **Efficient Access**: Fast asset retrieval and conversion to Arc<Bytes>
- **Memory Management**: Arc<Bytes> for shared asset access
- **Rust Embed Integration**: Static asset bundling at build time

## Features

### Asset Management
- **Compile-Time Embedding**: Assets embedded during compilation
- **Convenient Conversion**: Helper function to convert embedded assets to Arc<Bytes>
- **Shared References**: Arc<Bytes> for memory-efficient sharing
- **Path Prefixing**: Organized asset path structure

### Performance
- **Static Assets**: No runtime file I/O for embedded assets
- **Memory Efficient**: Shared references reduce memory usage
- **Fast Access**: Direct memory access to embedded data

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_native_image = { path = "../app/native/image" }
```

## Usage

### Asset Access

```rust
use moosicbox_app_native_image::{Asset, get_asset_arc_bytes};

// Get an embedded asset
let asset = Asset::get("/public/logo.png").unwrap();

// Convert to Arc<Bytes> for efficient sharing
let data = get_asset_arc_bytes(asset);

// Use the asset data
serve_image_response(data).await;
```

### Available Assets

Assets are embedded from the `../public/` directory with the `/public/` prefix.

## Build Configuration

### Asset Directory
Assets are embedded from: `$CARGO_MANIFEST_DIR/../public/`

### Prefix
All assets are accessible with the `/public/` prefix.

## Dependencies

- **Rust Embed**: Static asset embedding
- **Bytes**: Efficient byte buffer management
- **Standard Library**: Core functionality

## Integration

This package is designed for:
- **Native Desktop Apps**: Bundled image assets
- **Tauri Applications**: Static asset serving
- **Embedded GUIs**: Image asset management
- **Self-Contained Apps**: No external file dependencies
