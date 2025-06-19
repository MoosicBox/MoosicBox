# MoosicBox Music API API

API models and endpoint implementations for music API management.

## Overview

The MoosicBox Music API API package provides:

- **API Models**: REST API models for music API management
- **Authentication Models**: Authentication method definitions and values
- **API Conversion**: Convert MusicApi instances to API-compatible models
- **Feature Integration**: Optional API endpoint implementations

## Models

### ApiMusicApi
- **Service Info**: ID, name, and display information
- **Authentication Status**: Login status and authentication methods
- **Scan Support**: Scanning capabilities and status
- **Feature Flags**: Supported operations and configurations

### Authentication
- **AuthMethod**: Available authentication methods (UsernamePassword, Poll)
- **AuthValues**: Authentication credential structures
- **Login Status**: Real-time authentication status checking

## Features

### API Model Conversion
- **MusicApi to ApiMusicApi**: Convert service instances to API models
- **Authentication Integration**: Include authentication status and methods
- **Scan Status**: Include scanning capabilities and current status
- **Async Operations**: Async conversion with error handling

### Authentication Support
- **Username/Password**: Traditional credential-based authentication
- **Poll Authentication**: OAuth-style polling authentication
- **Status Checking**: Real-time authentication status verification

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_api_api = { path = "../music_api/api" }

# Enable API endpoints
moosicbox_music_api_api = {
    path = "../music_api/api",
    features = ["api"]
}

# Enable authentication features
moosicbox_music_api_api = {
    path = "../music_api/api",
    features = ["auth-username-password", "auth-poll"]
}
```

## Usage

### API Model Conversion

```rust
use moosicbox_music_api_api::models::convert_to_api_music_api;
use moosicbox_music_api::MusicApi;

// Convert MusicApi to API model
let api_model = convert_to_api_music_api(&*music_api).await?;

println!("Service: {}", api_model.name);
println!("Logged in: {}", api_model.logged_in);
println!("Supports scan: {}", api_model.supports_scan);
```

### Authentication Models

```rust
use moosicbox_music_api_api::models::{AuthMethod, AuthValues};

// Check authentication method
match api_model.auth_method {
    Some(AuthMethod::UsernamePassword) => {
        println!("Requires username and password");
    }
    Some(AuthMethod::Poll) => {
        println!("Uses polling authentication");
    }
    None => {
        println!("No authentication required");
    }
}

// Provide authentication values
let auth_values = AuthValues::UsernamePassword {
    username: "user".to_string(),
    password: "pass".to_string(),
};
```

### Service Information

```rust
// Access service information
println!("Service ID: {}", api_model.id);
println!("Display Name: {}", api_model.name);
println!("Authentication Status: {}", api_model.logged_in);
println!("Scan Support: {}", api_model.supports_scan);
println!("Scan Enabled: {}", api_model.scan_enabled);
```

## Feature Flags

- **`api`**: Enable API endpoint implementations
- **`auth-username-password`**: Enable username/password authentication
- **`auth-poll`**: Enable polling-based authentication
- **`openapi`**: Enable OpenAPI schema generation

## Dependencies

- **MoosicBox Music API**: Core music API traits
- **Serde**: Serialization and deserialization
- **UToipa**: Optional OpenAPI schema generation

## Integration

This package is designed for:
- **REST API Development**: API model definitions for endpoints
- **Authentication Systems**: Authentication method handling
- **Service Management**: Music service configuration and status
- **API Documentation**: OpenAPI schema generation
