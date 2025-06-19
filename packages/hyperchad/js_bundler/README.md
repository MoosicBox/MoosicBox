# HyperChad JavaScript Bundler

JavaScript bundling and compilation utilities for HyperChad applications.

## Overview

The HyperChad JavaScript Bundler package provides:

- **Multiple Bundlers**: Support for ESBuild and SWC bundlers
- **Node.js Integration**: Node.js runtime utilities
- **Feature-Gated**: Modular bundler selection
- **Build Pipeline**: JavaScript compilation and bundling

## Features

### Bundler Support
- **ESBuild**: Fast JavaScript bundler and minifier
- **SWC**: Rust-based JavaScript/TypeScript compiler
- **Pluggable**: Choose bundler based on requirements
- **Performance**: High-performance bundling options

### Node.js Integration
- **Runtime**: Node.js runtime utilities
- **Process Management**: Node process handling
- **Environment**: Node environment configuration

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_js_bundler = { path = "../hyperchad/js_bundler" }

# Enable specific bundlers
hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["esbuild"]
}

hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["swc"]
}

# Enable Node.js integration
hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["node"]
}
```

## Usage

### ESBuild Integration (with `esbuild` feature)

```rust
use hyperchad_js_bundler::esbuild;

// ESBuild bundling operations
// (Implementation depends on enabled features)
```

### SWC Integration (with `swc` feature)

```rust
use hyperchad_js_bundler::swc;

// SWC compilation operations
// (Implementation depends on enabled features)
```

### Node.js Integration (with `node` feature)

```rust
use hyperchad_js_bundler::node;

// Node.js runtime operations
// (Implementation depends on enabled features)
```

### General Bundler Usage

```rust
use hyperchad_js_bundler::*;

// Use bundler functionality
// (Available functions depend on enabled features)
```

## Feature Flags

- **`esbuild`**: Enable ESBuild bundler support
- **`swc`**: Enable SWC compiler support
- **`node`**: Enable Node.js runtime integration

## Bundler Comparison

### ESBuild
- **Speed**: Extremely fast bundling
- **JavaScript/TypeScript**: Full support
- **Minification**: Built-in minification
- **Tree Shaking**: Advanced dead code elimination

### SWC
- **Rust-based**: Written in Rust for performance
- **TypeScript**: Native TypeScript support
- **Transforms**: Advanced code transformations
- **Plugins**: Extensible plugin system

## Dependencies

Dependencies vary based on enabled features:
- **ESBuild**: ESBuild JavaScript bundler
- **SWC**: SWC Rust compiler
- **Node**: Node.js runtime libraries

## Integration

This package is designed for:
- **Build Systems**: JavaScript build pipeline integration
- **Development Tools**: Development server bundling
- **Production Builds**: Optimized production bundling
- **HyperChad Apps**: JavaScript bundling for HyperChad applications

## Note

This package provides a modular approach to JavaScript bundling. Enable only the features you need to minimize dependencies and build times. The actual bundling implementations are feature-gated and will only be available when the corresponding features are enabled.
