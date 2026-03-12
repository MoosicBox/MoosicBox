# MoosicBox Scan Models

Data models for music library scanning and indexing operations.

## Overview

The MoosicBox Scan Models package provides:

- **API Integration**: REST-compatible scan data models

## Features

Currently implemented:

- `ApiScanPath`: Model for representing scan paths in API requests

## Installation

Add this to your Cargo.toml:

```toml
[dependencies]
moosicbox_scan_models = { path = "../scan/models" }
```

Enable optional features:

```toml
[dependencies]
moosicbox_scan_models = { path = "../scan/models", features = ["openapi"] }
```

Available features:

- `api`: API models (enabled by default)
- `openapi`: OpenAPI schema support via utoipa (enabled by default)

## Usage

The primary API model is `ApiScanPath` in the `api` module:

```rust
use moosicbox_scan_models::api::ApiScanPath;

let scan_path = ApiScanPath {
    path: "/music".to_string(),
};
```

## Dependencies

- **Serde**: Serialization and deserialization
- **log**: Logging facade
- **moosicbox_assert**: Assertion utilities
- **utoipa**: OpenAPI schema generation (optional, enabled with `openapi` feature)
