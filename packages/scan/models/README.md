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

## Dependencies

- **Serde**: Serialization and deserialization
- **utoipa**: OpenAPI schema generation (optional, enabled with `openapi` feature)
- **log**: Logging functionality
