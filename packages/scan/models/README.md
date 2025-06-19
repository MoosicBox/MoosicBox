# MoosicBox Scan Models

Data models for music library scanning and indexing operations.

## Overview

The MoosicBox Scan Models package provides:

- **Scan Configuration**: Library scan setup and parameters
- **Progress Tracking**: Scan progress and status models
- **File Discovery**: Scanned file and metadata structures
- **API Integration**: REST-compatible scan data models

## Installation

Add this to your Cargo.toml:

[dependencies]
moosicbox_scan_models = { path = "../scan/models" }

## Dependencies

- **Serde**: Serialization and deserialization
- **MoosicBox Core Models**: Core music and file types
