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
- **ESBuild**: Fast JavaScript bundler and minifier via external esbuild binary
- **SWC**: Rust-based JavaScript/TypeScript compiler with full bundling implementation
- **Pluggable**: Choose bundler based on requirements via feature flags
- **Performance**: High-performance bundling options

### Node.js Integration
- **Command Execution**: Node.js and npm package manager command execution
- **Multi-toolchain Support**: Supports npm, pnpm, and bun (feature-gated)
- **Process Management**: Automatic binary selection and process handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_js_bundler = { path = "../hyperchad/js_bundler" }

# Enable specific bundlers (default enables both esbuild and swc)
hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["esbuild"]  # Requires node feature
}

hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["swc"]
}

# Enable specific package managers
hyperchad_js_bundler = {
    path = "../hyperchad/js_bundler",
    features = ["npm"]  # or "pnpm", "bun"
}
```

**Note:** The default features include `all-web-toolchains`, `esbuild`, and `swc`.

## Usage

### Unified Bundler API

The package provides a unified `bundle` function that dispatches to the appropriate bundler:

```rust
use hyperchad_js_bundler::bundle;
use std::path::Path;

// Bundle JavaScript/TypeScript file
// Uses SWC if available, falls back to ESBuild
bundle(
    Path::new("src/index.js"),
    Path::new("dist/bundle.js")
);
```

### ESBuild Integration (with `esbuild` feature)

ESBuild bundles by executing the external esbuild binary via npm:

```rust
use hyperchad_js_bundler::esbuild;
use std::path::Path;

// Bundle with ESBuild (runs npm install and esbuild binary)
esbuild::bundle(
    Path::new("src/index.js"),
    Path::new("dist/bundle.js")
);
```

### SWC Integration (with `swc` feature)

SWC provides full Rust-based bundling with minification support:

```rust
use hyperchad_js_bundler::swc;
use std::path::Path;

// Bundle with SWC (minify: true)
swc::bundle(
    Path::new("src/index.ts"),
    Path::new("dist/bundle.js"),
    true  // minify
);
```

The SWC bundler supports:
- TypeScript and JavaScript files
- Minification and code optimization
- Tree shaking and dead code elimination
- Module resolution via Node.js resolver
- Import meta URL handling

### Node.js Integration (with `node` feature)

Execute npm/pnpm/bun commands:

```rust
use hyperchad_js_bundler::node::{run_npm_command, run_command};
use std::path::Path;

// Run npm command (tries pnpm, bun, npm in order based on enabled features)
run_npm_command(
    &["install"],
    Path::new(".")
);

// Run custom commands with binary fallback
run_command(
    ["pnpm", "bun", "npm"].iter().map(|s| s.to_string()),
    &["run", "build"],
    Path::new(".")
);
```

## Feature Flags

### Bundlers
- **`esbuild`**: Enable ESBuild bundler support (requires `node` feature)
- **`swc`**: Enable SWC compiler support

### Package Managers
- **`node`**: Base feature for Node.js command execution
- **`npm`**: Enable npm package manager support (enables `node`)
- **`pnpm`**: Enable pnpm package manager support (enables `node`)
- **`bun`**: Enable bun package manager support (enables `node`)
- **`all-web-toolchains`**: Enable all package managers (npm, pnpm, bun)

### Other
- **`fail-on-warnings`**: Treat warnings as errors
- **`default`**: Includes `all-web-toolchains`, `esbuild`, and `swc`

## Bundler Comparison

### ESBuild
- **Implementation**: Executes external esbuild binary via npm
- **Speed**: Extremely fast bundling and minification
- **Dependencies**: Requires npm/pnpm/bun installation
- **Features**: Automatic minification and bundling via command-line flags

### SWC
- **Implementation**: Fully integrated Rust-based bundler
- **Speed**: Fast Rust-native bundling
- **TypeScript**: Native TypeScript stripping and compilation
- **Features**:
  - Configurable minification with compress and mangle options
  - Module resolution via Node.js resolver with caching
  - Dead code elimination (DCE)
  - ES module output
  - Import meta property handling

## Dependencies

Core dependencies (always included):
- **log**: Logging facade
- **switchy_env**: Environment variable utilities

Feature-gated dependencies:
- **SWC feature**: Includes swc_bundler, swc_common, swc_ecma_* crates, and anyhow
- **ESBuild feature**: No Rust dependencies (uses external binary)
- **Node feature**: No additional dependencies (command execution only)

External requirements:
- **ESBuild**: Requires npm/pnpm/bun and esbuild package installation
- **SWC**: No external requirements (fully Rust-based)

## Integration

This package is designed for:
- **Build Systems**: JavaScript build pipeline integration
- **Development Tools**: Development server bundling
- **Production Builds**: Optimized production bundling
- **HyperChad Apps**: JavaScript bundling for HyperChad applications

## Module Structure

The package consists of the following modules:

- **`bundler.rs`**: Unified bundler interface that dispatches to SWC or ESBuild
- **`esbuild.rs`**: ESBuild integration via external binary execution
- **`swc.rs`**: Full SWC bundler implementation with custom loader and hooks
- **`node.rs`**: Command execution utilities for npm/pnpm/bun
- **`lib.rs`**: Feature-gated module exports

## Note

This package provides a modular approach to JavaScript bundling. Enable only the features you need to minimize dependencies and build times. The actual bundling implementations are feature-gated and will only be available when the corresponding features are enabled.

The unified `bundle()` function prioritizes SWC over ESBuild when both features are enabled, as SWC provides a fully integrated Rust-based solution without external binary dependencies.
